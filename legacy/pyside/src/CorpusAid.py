import copy
import json
import logging
import multiprocessing
import os
import re
import shutil
import sys
import threading
import time
import unicodedata
import urllib.request
from io import StringIO
from threading import Lock

from corpusaid.ingestion import (
    UnsupportedDocumentTypeError,
    can_overwrite_with_extracted_text,
    extract_document,
    load_document_preview,
    scan_supported_paths,
    supported_file_extensions,
)
from corpusaid.exporting import (
    ProcessedDocumentRecord,
    export_processed_corpus as write_processed_corpus_export,
)

try:
    from corpus_preview import generate_report_summary as rust_generate_report_summary
except ImportError:  # pragma: no cover - fallback when extension is unavailable
    rust_generate_report_summary = None

from PySide6.QtCore import (
    QObject,
    QRegularExpression,
    QRunnable,
    QSize,
    Qt,
    QThread,
    QThreadPool,
    QTimer,
    QUrl,
    Signal,
    Slot,
)
from PySide6.QtGui import (
    QAction,
    QColor,
    QDesktopServices,
    QFont,
    QFontDatabase,
    QIcon,
    QIntValidator,
    QKeySequence,
    QPainter,
    QSyntaxHighlighter,
    QTextCharFormat,
    QTextCursor,
)
from PySide6.QtWebEngineWidgets import QWebEngineView
from PySide6.QtWidgets import (
    QApplication,
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QGridLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMainWindow,
    QMessageBox,
    QPlainTextEdit,
    QProgressBar,
    QPushButton,
    QSizePolicy,
    QSplitter,
    QSpinBox,
    QStackedWidget,
    QStatusBar,
    QStyle,
    QTableWidget,
    QTabWidget,
    QTextEdit,
    QToolBar,
    QToolButton,
    QVBoxLayout,
    QWidget,
    QWizard,
    QWizardPage,
)


class ErrorHandler:
    @staticmethod
    def show_error(message, title="Error", parent=None):
        logging.error(message)
        QMessageBox.warning(parent, title, message)

    @staticmethod
    def show_warning(message, title="Warning", parent=None):
        logging.warning(message)
        QMessageBox.warning(parent, title, message)


class AppSignals(QObject):
    result = Signal(object, str, float)  # document, processed_text, processing_time
    error = Signal(str, str)  # file_path, error_message
    warning = Signal(str, str)  # file_path, warning_message
    finished = Signal()
    update_progress = Signal(int, int, float, str)  # current, total, time_remaining, error
    processing_complete = Signal(list, list)  # processed_results, warnings


# resource_path function to get the path of the resource file
def resource_path(relative_path):
    """Get absolute path to resource, works for development and PyInstaller builds."""
    if getattr(sys, "frozen", False):  # Check if running as a PyInstaller bundle
        base_path = getattr(sys, "_MEIPASS", os.path.dirname(sys.executable))
    else:
        base_path = os.path.dirname(os.path.abspath(__file__))
    return os.path.join(base_path, relative_path)


def _log_ingestion_warnings(file_path, warnings):
    for warning in warnings:
        logging.warning("Document ingestion warning for %s: %s", file_path, warning)


def load_extracted_document(file_path):
    """Load full document content and provenance through the ingestion layer."""
    try:
        return extract_document(file_path)
    except UnsupportedDocumentTypeError as exc:
        logging.error("Unsupported document type for %s: %s", file_path, exc)
        return None
    except Exception as exc:
        logging.error("Error loading %s: %s", file_path, exc)
        return None


def load_extracted_text(file_path):
    """Load full file content through the document ingestion layer."""
    document = load_extracted_document(file_path)
    if document is None:
        return ""

    _log_ingestion_warnings(file_path, document.warnings)
    return document.text


def load_text_with_rust_fallback(file_path):
    """Compatibility wrapper for older call sites."""
    return load_extracted_text(file_path)


def load_extracted_preview(file_path, limit):
    """Load a text preview through the document ingestion layer."""
    try:
        preview = load_document_preview(file_path, limit)
    except UnsupportedDocumentTypeError as exc:
        logging.error("Unsupported document type for %s: %s", file_path, exc)
        return "", False
    except Exception as exc:
        logging.error("Error loading preview for %s: %s", file_path, exc)
        return "", False

    _log_ingestion_warnings(file_path, preview.warnings)
    return preview.text, preview.truncated


def supported_file_dialog_filter():
    patterns = " ".join(f"*{extension}" for extension in supported_file_extensions())
    return f"Supported Documents ({patterns})"


def is_supported_document_path(file_path):
    extension = os.path.splitext(file_path)[1].lower()
    return os.path.isfile(file_path) and extension in supported_file_extensions()


def processed_text_export_path(output_directory, source_path, reserved_paths):
    base_name = os.path.splitext(os.path.basename(source_path))[0] or "document"
    candidate = os.path.join(output_directory, f"{base_name}.txt")
    index = 2

    def normalized(path):
        return os.path.normcase(os.path.abspath(path))

    while normalized(candidate) in reserved_paths or os.path.exists(candidate):
        candidate = os.path.join(output_directory, f"{base_name}-{index}.txt")
        index += 1

    reserved_paths.add(normalized(candidate))
    return candidate


def processed_corpus_record_from_document(document):
    """Build a PySide-free export record from a GUI document object."""
    document.ensure_extraction_metadata()
    original_text = document._original_text
    return ProcessedDocumentRecord(
        source_path=document.file_path,
        original_text=original_text,
        processed_text=document.processed_text,
        document_type=document._document_type,
        extraction_method=document._extraction_method,
        warnings=list(document._extraction_warnings),
        metadata=dict(document._extraction_metadata),
        extracted_character_count=(len(original_text) if original_text is not None else None),
    )


# Usage of resource_path updated to the new folder structure:
# For assets
icon_path = resource_path("assets/my_icon.ico")

# For documentation
documentation_path = resource_path("docs/documentation.html")

PREVIEW_CHAR_LIMIT = 5000
MAX_DISPLAY_FILES = 5000
PREVIEW_TRUNCATION_MARKER = "\n\n[Preview truncated. Open the file to view the full contents.]"
PREVIEW_BATCH_SIZE = 20


def get_spacy_model():
    import spacy

    if not hasattr(get_spacy_model, "lock"):
        get_spacy_model.lock = threading.Lock()
    with get_spacy_model.lock:
        if not hasattr(get_spacy_model, "nlp"):
            get_spacy_model.nlp = spacy.load("en_core_web_sm", disable=["parser", "ner"])
            get_spacy_model.nlp.add_pipe("sentencizer")
            get_spacy_model.nlp.max_length = 200000000
    return get_spacy_model.nlp


def load_fonts():
    font_db = QFontDatabase()
    roboto_font_path = resource_path("assets/fonts/Roboto-Regular.ttf")
    if os.path.exists(roboto_font_path):
        font_id = font_db.addApplicationFont(roboto_font_path)
        if font_id != -1:
            font_families = font_db.applicationFontFamilies(font_id)
            if font_families:
                QFont("Roboto")
    else:
        logging.warning("Roboto font not found. Falling back to default font.")


class PreprocessingModule:
    def process(self, text):
        raise NotImplementedError


class CharacterFilterModule(PreprocessingModule):
    def __init__(self, chars_to_remove):
        logging.debug(f"Initializing CharacterFilterModule with: {chars_to_remove}")
        # Escape special regex characters
        escaped_sequences = [re.escape(seq) for seq in chars_to_remove]
        logging.debug(f"Escaped sequences: {escaped_sequences}")
        patterns = []
        for seq in escaped_sequences:
            if re.match(r"^\w+$", seq):  # Sequence contains only word characters
                patterns.append(f"\\b{seq}\\b")
            else:
                patterns.append(seq)
        pattern = "|".join(patterns)
        logging.debug(f"Final regex pattern: {pattern}")
        try:
            self.pattern = re.compile(pattern, re.IGNORECASE)  # Case-insensitive
            logging.debug("Compiled regex pattern successfully.")
        except re.error as e:
            logging.error(f"Failed to compile regex pattern: {e}")
            self.pattern = None

    def process(self, text):
        if not self.pattern:
            logging.warning("No valid regex pattern. Skipping CharacterFilterModule.")
            return text
        logging.debug("Starting CharacterFilterModule processing.")
        result = self.pattern.sub("", text)
        logging.debug("Completed CharacterFilterModule processing.")
        return result


class LineBreakNormalizationModule(PreprocessingModule):
    def __init__(self):
        # Match lines that contain only a single word character (typical OCR artefacts)
        self.single_char_line_pattern = re.compile(r"\s*\w\s*")
        self.line_break_pattern = re.compile(r"(?<!\.\s)\n(?!\s*\n)", re.MULTILINE)

    def process(self, text):
        lines = text.splitlines()
        filtered_lines = [
            line for line in lines if not self.single_char_line_pattern.fullmatch(line)
        ]
        text = "\n".join(filtered_lines)
        text = self.line_break_pattern.sub(" ", text)
        text = re.sub(r"\s{2,}", " ", text)
        return text.strip()


class PageNumberRemovalModule(PreprocessingModule):
    def __init__(self):
        self.pattern = re.compile(r"^\s*\d{1,4}\s*$", re.MULTILINE)

    def process(self, text):
        return self.pattern.sub("", text)


class RomanPageNumberRemovalModule(PreprocessingModule):
    def __init__(self):
        self.pattern = re.compile(
            r"^\s*"  # Start of line with optional whitespace
            r"(?P<roman>[IiVvXxLlCcDdMm]{1,7})"  # Roman numerals (both cases)
            r"\s*$",  # End of line with optional whitespace
            re.MULTILINE,
        )

    def process(self, text):
        return self.pattern.sub("", text)


class PageIndicatorRemovalModule(PreprocessingModule):
    def __init__(self):
        self.pattern = re.compile(
            r"\b(?:[Pp]age|[Pp]ag\.?)\s+" r"(?P<number>\d+|[IVXLCDM]+)\b", re.MULTILINE
        )

    def process(self, text):
        return self.pattern.sub("", text)


class PageDelimiterRemovalModule(PreprocessingModule):
    def __init__(self):
        # Pattern matches "--- Page X ---" where X is any number
        self.pattern = re.compile(r"---\s*Page\s+\d+\s*---", re.MULTILINE)

    def process(self, text):
        return self.pattern.sub("", text)


class WhitespaceNormalizationModule(PreprocessingModule):
    def process(self, text):
        text = re.sub(r"\s+([.,?!;:])", r"\1", text)  # Remove whitespace before punctuation
        text = re.sub(r"([.,?!;:])(\S)", r"\1 \2", text)  # Add whitespace after punctuation
        text = re.sub(r"\(\s+", "(", text)  # Remove whitespace after opening parenthesis
        text = re.sub(r"\s+\)", ")", text)  # Remove whitespace before closing parenthesis
        text = re.sub(r"\[\s+", "[", text)  # Remove whitespace after opening bracket
        text = re.sub(r"\s+\]", "]", text)  # Remove whitespace before closing bracket
        text = re.sub(r"\{\s+", "{", text)  # Remove whitespace after opening brace
        text = re.sub(r"\s+\}", "}", text)  # Remove whitespace before closing brace
        text = re.sub(r"\s{2,}", " ", text)  # Replace multiple spaces with a single space
        return text.strip()


class LineBreakRemovalModule(PreprocessingModule):
    def process(self, text):
        return text.replace("\n", " ")


class BibliographicalReferenceRemovalModule(PreprocessingModule):
    def __init__(self):
        self.pattern = re.compile(r"\([A-Z][a-z]+(?:[^()]*?\d{4}[^()]*?)?\)")

    def process(self, text: str) -> str:
        return self.pattern.sub("", text)


class LowercaseModule(PreprocessingModule):
    def process(self, text):
        return text.lower()


class TokenBasedModule(PreprocessingModule):
    def __init__(self):
        self.nlp = get_spacy_model()

    def process_tokens(self, tokens):
        raise NotImplementedError

    def process(self, text_or_tokens):
        if isinstance(text_or_tokens, str):
            doc = self.nlp(text_or_tokens)
            tokens = [token.text for token in doc]
        else:
            tokens = text_or_tokens
        return self.process_tokens(tokens)


class RegexSubstitutionModule(PreprocessingModule):
    def __init__(self, pattern, replacement=""):
        try:
            self.pattern = re.compile(pattern, re.DOTALL)
            logging.debug(f"Compiled regex pattern: {pattern}")
        except re.error as e:
            logging.error(f"Invalid regex pattern: {pattern}\nError: {str(e)}")
            ErrorHandler.show_error(
                f"The entered regex pattern is invalid:\n{str(e)}",
                "Invalid Regex Pattern",
                parent=None,
            )
            self.pattern = None
        self.replacement = replacement

    def process(self, text):
        if self.pattern:
            new_text, count = self.pattern.subn(self.replacement, text)
            if count > 0:
                logging.debug(
                    f"Applied pattern: '{self.pattern.pattern}' - {count} replacements made."
                )
            return new_text
        return text


class WordTokenizationModule(TokenBasedModule):
    def process(self, text):
        doc = self.nlp(text)
        tokens = [token.text for token in doc]
        return " ".join(tokens)


class StopWordRemovalModule(TokenBasedModule):
    def __init__(self):
        super().__init__()
        from spacy.lang.en.stop_words import STOP_WORDS

        self.stop_words = set(STOP_WORDS)

    def process_tokens(self, tokens):
        return [word for word in tokens if word.lower() not in self.stop_words]


class HTMLStripperModule(PreprocessingModule):
    def process(self, text):
        from bs4 import BeautifulSoup

        return BeautifulSoup(text, "html.parser").get_text()


class DiacriticRemovalModule(PreprocessingModule):
    def process(self, text):
        return "".join(
            c for c in unicodedata.normalize("NFD", text) if unicodedata.category(c) != "Mn"
        )


class GreekLetterRemovalModule(PreprocessingModule):
    def process(self, text):
        return "".join(
            char for char in text if not unicodedata.name(char, "").startswith("GREEK")
        )


class CyrillicRemovalModule(PreprocessingModule):
    def process(self, text):
        return "".join(
            char for char in text if not unicodedata.name(char, "").startswith("CYRILLIC")
        )


class UnicodeNormalizationModule(PreprocessingModule):
    def process(self, text):
        return unicodedata.normalize("NFKC", text)


class UnicodeCategoryFilterModule(PreprocessingModule):
    def __init__(self, categories_to_remove):
        self.categories_to_remove = set(categories_to_remove)

    def process(self, text):
        return "".join(
            char for char in text if unicodedata.category(char) not in self.categories_to_remove
        )


class PreprocessingPipeline:
    def __init__(self):
        self.modules = []

    def add_module(self, module):
        self.modules.append(module)

    def process(self, text):
        for module in self.modules:
            text = module.process(text)
            if isinstance(text, list):
                text = " ".join(text)
        return text.strip()


class Document:
    def __init__(self, file_path):
        self.file_path = os.path.normpath(file_path)
        self._original_text = None
        self._processed_text = None
        self._document_type = None
        self._extraction_method = None
        self._extraction_warnings = []
        self._extraction_metadata = {}
        self.is_modified = False
        self.history = []
        self.history_index = -1

    def load_original_text(self):
        if self._original_text is not None:
            return self._original_text

        extracted_document = load_extracted_document(self.file_path)
        if extracted_document is None:
            self._original_text = ""
        else:
            _log_ingestion_warnings(self.file_path, extracted_document.warnings)
            self._cache_extracted_document(extracted_document)

        if self._processed_text is None:
            self._processed_text = self._original_text
        if not self.history:
            self.history = [self._processed_text]
            self.history_index = 0

        return self._original_text

    def _cache_extracted_document(self, extracted_document):
        self._original_text = extracted_document.text
        self._document_type = extracted_document.document_type
        self._extraction_method = extracted_document.extraction_method
        self._extraction_warnings = list(extracted_document.warnings)
        self._extraction_metadata = dict(extracted_document.metadata)

    def ensure_extraction_metadata(self):
        if self._extraction_method is not None and self._original_text is not None:
            return

        extracted_document = load_extracted_document(self.file_path)
        if extracted_document is None:
            if self._original_text is None:
                self._original_text = ""
            return

        _log_ingestion_warnings(self.file_path, extracted_document.warnings)
        existing_processed_text = self._processed_text
        self._cache_extracted_document(extracted_document)
        if existing_processed_text is not None:
            self._processed_text = existing_processed_text

    @property
    def original_text(self):
        return self.load_original_text()

    @property
    def processed_text(self):
        if self._processed_text is None:
            self._processed_text = self.load_original_text()
        return self._processed_text

    @staticmethod
    def _truncate_text(text, limit):
        if limit is None or limit <= 0:
            return text, False
        if len(text) > limit:
            return text[:limit], True
        return text, False

    def get_original_preview(self, limit):
        if self._original_text is not None:
            return self._truncate_text(self._original_text, limit)

        if limit is None or limit <= 0:
            return self.original_text, False

        preview_text, truncated = load_extracted_preview(self.file_path, limit)

        if not truncated:
            # Cache the full content since it fits within the preview limit
            self._original_text = preview_text
            if self._processed_text is None:
                self._processed_text = preview_text
            if not self.history:
                self.history = [preview_text]
                self.history_index = 0

        return preview_text, truncated

    def get_processed_preview(self, limit):
        if self._processed_text is None:
            return self.get_original_preview(limit)
        if limit is None or limit <= 0:
            return self.processed_text, False
        return self._truncate_text(self._processed_text, limit)

    def update_processed_text(self, new_text):
        if new_text != self.processed_text:
            self._processed_text = new_text
            self.is_modified = True
            if self.history_index < len(self.history) - 1:
                self.history = self.history[: self.history_index + 1]
            self.history.append(self._processed_text)
            self.history_index += 1

    def undo(self):
        if self.history_index > 0:
            self.history_index -= 1
            self._processed_text = self.history[self.history_index]
            self.is_modified = self._processed_text != self.original_text
            logging.debug(f"Undo performed. Current history index: {self.history_index}")

    def redo(self):
        if self.history_index < len(self.history) - 1:
            self.history_index += 1
            self._processed_text = self.history[self.history_index]
            self.is_modified = self._processed_text != self.original_text
            logging.debug(f"Redo performed. Current history index: {self.history_index}")


class DocumentProcessor:
    default_parameters = {
        "remove_break_lines": False,
        "lowercase": False,
        "chars_to_remove": [],
        "word_tokenization": False,
        "remove_stop_words": False,
        "regex_pattern": "",
        "strip_html": False,
        "remove_diacritics": False,
        "remove_greek": False,
        "remove_cyrillic": False,
        "remove_super_sub_script": False,
        "remove_roman_page_numbers": False,
        "remove_page_indicators": False,
        "remove_page_numbers": False,
        "remove_page_delimiters": False,
        "remove_bibliographical_references": False,
        "normalize_spacing": False,
        "normalize_unicode": False,
        "normalize_line_breaks": False,
    }

    def __init__(self):
        self.parameters = copy.deepcopy(self.default_parameters)
        self.update_pipeline()

    def set_parameters(self, parameters):
        try:
            if "regex_pattern" in parameters and parameters["regex_pattern"]:
                re.compile(parameters["regex_pattern"])
            if "chars_to_remove" in parameters:
                if not isinstance(parameters["chars_to_remove"], list):
                    raise ValueError("chars_to_remove must be a list of strings")
                for item in parameters["chars_to_remove"]:
                    if not isinstance(item, str):
                        raise ValueError("All items in chars_to_remove must be strings")
            sanitized_parameters = copy.deepcopy(parameters)
            self.parameters.update(sanitized_parameters)
            self.update_pipeline()
            logging.debug(f"Updated parameters: {self.parameters}")
        except re.error as e:
            logging.error(f"Invalid regex pattern: {e}")
            ErrorHandler.show_error(
                f"The entered regex pattern is invalid:\n{str(e)}",
                "Invalid Regex Pattern",
                parent=None,
            )
        except ValueError as e:
            logging.error(f"Parameter validation error: {e}")
            ErrorHandler.show_error(str(e), "Parameter Error", parent=None)

    def reset_parameters(self):
        self.parameters = copy.deepcopy(self.default_parameters)
        self.update_pipeline()

    def update_pipeline(self):
        self.pipeline = PreprocessingPipeline()

        # **1. Normalization Modules**
        if self.parameters["normalize_unicode"]:
            self.pipeline.add_module(UnicodeNormalizationModule())
        if self.parameters["remove_diacritics"]:
            self.pipeline.add_module(DiacriticRemovalModule())
        if self.parameters["normalize_spacing"]:
            self.pipeline.add_module(WhitespaceNormalizationModule())
        if self.parameters["normalize_line_breaks"]:
            self.pipeline.add_module(LineBreakNormalizationModule())
        if self.parameters["remove_break_lines"]:
            self.pipeline.add_module(LineBreakRemovalModule())

        # **2. Removal Modules**
        if self.parameters["chars_to_remove"]:
            self.pipeline.add_module(CharacterFilterModule(self.parameters["chars_to_remove"]))
        if self.parameters["remove_page_numbers"]:
            self.pipeline.add_module(PageNumberRemovalModule())
        if self.parameters["remove_roman_page_numbers"]:
            self.pipeline.add_module(RomanPageNumberRemovalModule())
        if self.parameters["remove_page_indicators"]:
            self.pipeline.add_module(PageIndicatorRemovalModule())
        if self.parameters["remove_page_delimiters"]:
            self.pipeline.add_module(PageDelimiterRemovalModule())
        if self.parameters["remove_bibliographical_references"]:
            self.pipeline.add_module(BibliographicalReferenceRemovalModule())

        # **3. Transformation Modules**
        if self.parameters["lowercase"]:
            self.pipeline.add_module(LowercaseModule())
        if self.parameters["strip_html"]:
            self.pipeline.add_module(HTMLStripperModule())

        # **4. Tokenization and Filtering**
        if self.parameters["word_tokenization"]:
            self.pipeline.add_module(WordTokenizationModule())
        if self.parameters["remove_stop_words"]:
            self.pipeline.add_module(StopWordRemovalModule())

        # **5. Character Set Removal**
        if self.parameters["remove_greek"]:
            self.pipeline.add_module(GreekLetterRemovalModule())
        if self.parameters["remove_cyrillic"]:
            self.pipeline.add_module(CyrillicRemovalModule())
        if self.parameters["remove_super_sub_script"]:
            categories_to_remove = {"No", "Sk"}
            self.pipeline.add_module(UnicodeCategoryFilterModule(categories_to_remove))

        # **6. Regex Substitution**
        if self.parameters["regex_pattern"]:
            self.pipeline.add_module(RegexSubstitutionModule(self.parameters["regex_pattern"]))

        # **7. Final Normalizations (if any)**
        # Removed redundant additions

    def get_parameters(self):
        return self.parameters

    def process_file(self, text):
        if not any(self.parameters.values()):
            return text
        processed_text = self.pipeline.process(text)
        logging.debug("Processed text through pipeline.")
        return processed_text.strip()


class ProcessingWorker(QRunnable):
    def __init__(self, processor, document, signals):
        super().__init__()
        self.processor = processor
        self.document = document
        self.signals = signals
        self.setAutoDelete(True)

    def run(self):
        try:
            start_time = time.time()
            processed_text = self.processor.process_file(self.document.original_text)
            processing_time = time.time() - start_time
            self.signals.result.emit(
                self.document, processed_text, processing_time
            )  # 3 arguments
        except Exception as e:
            self.signals.error.emit(
                self.document.file_path, f"Unexpected error during processing: {str(e)}"
            )
        finally:
            self.signals.finished.emit()


class FileManager:
    def __init__(self):
        self.documents = []

    def add_files(self, file_paths):
        new_documents = []
        for path in file_paths:
            normalized_path = os.path.normpath(path)
            if not self.get_document_by_path(normalized_path):
                doc = Document(normalized_path)
                self.documents.append(doc)
                new_documents.append(doc)
        return new_documents

    def add_directory(self, directory, signals, is_cancelled_callback):
        new_documents = []
        try:
            candidate_paths = scan_supported_paths(directory)
        except Exception as exc:
            logging.error("Error scanning directory %s: %s", directory, exc)
            candidate_paths = []

        if not candidate_paths:
            signals.update_progress.emit(0, 0, 0, None)
            return []

        total_files = len(candidate_paths)
        start_time = time.time()
        processed_files = 0

        for raw_path in candidate_paths:
            if is_cancelled_callback():
                signals.update_progress.emit(
                    processed_files, total_files, 0, "Operation cancelled."
                )
                return []

            normalized_path = os.path.normpath(raw_path)
            if self.get_document_by_path(normalized_path):
                continue

            try:
                doc = Document(normalized_path)
                self.documents.append(doc)
                new_documents.append(doc)
                processed_files += 1
                elapsed_time = time.time() - start_time
                estimated_remaining_time = (
                    (elapsed_time / processed_files) * (total_files - processed_files)
                    if processed_files > 0
                    else 0
                )
                signals.update_progress.emit(
                    processed_files,
                    total_files,
                    estimated_remaining_time,
                    None,
                )
            except OSError as e:
                signals.update_progress.emit(
                    processed_files, total_files, 0, f"OS error: {str(e)}"
                )
            except Exception as e:
                signals.update_progress.emit(
                    processed_files,
                    total_files,
                    0,
                    f"Unexpected error: {str(e)}",
                )
        return new_documents

    def remove_files(self, file_paths):
        self.documents = [doc for doc in self.documents if doc.file_path not in file_paths]

    def clear_files(self):
        self.documents.clear()

    def get_files(self):
        return self.documents

    def get_document_by_path(self, path):
        for doc in self.documents:
            if doc.file_path == path:
                return doc
        return None

    def get_total_size(self):
        return sum(os.path.getsize(doc.file_path) for doc in self.documents)


class FileListWidget(QListWidget):
    files_added = Signal(list)
    files_removed = Signal(list)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setAcceptDrops(True)
        self.setSelectionMode(QListWidget.ExtendedSelection)
        self.setDragDropMode(QListWidget.InternalMove)

    def dragEnterEvent(self, event):
        if event.mimeData().hasUrls():
            event.acceptProposedAction()
        else:
            super().dragEnterEvent(event)

    def dragMoveEvent(self, event):
        if event.mimeData().hasUrls():
            event.setDropAction(Qt.CopyAction)
            event.accept()
        else:
            super().dragMoveEvent(event)

    def dropEvent(self, event):
        if event.mimeData().hasUrls():
            event.setDropAction(Qt.CopyAction)
            event.accept()
            links = []
            for url in event.mimeData().urls():
                file_path = os.path.normpath(url.toLocalFile())
                if is_supported_document_path(file_path):
                    links.append(file_path)
            if links:
                self.addItems(links)
                self.files_added.emit(links)
        else:
            super().dropEvent(event)
            source_row = self.currentRow()
            destination_row = self.row(self.itemAt(event.position().toPoint()))
            if source_row != destination_row:
                item = self.takeItem(source_row)
                self.insertItem(destination_row, item)

    def keyPressEvent(self, event):
        if event.key() == Qt.Key_Delete:
            selected_items = self.selectedItems()
            for item in selected_items:
                self.takeItem(self.row(item))
            self.files_removed.emit([item.text() for item in selected_items])
        else:
            super().keyPressEvent(event)


class ThemeManager:
    def __init__(self):
        self.dark_theme = True
        self.custom_colors = {
            "primary": "#518FBC",
            "secondary": "#325F84",
            "background": "#1E1E1E",
            "text": "#FFFFFF",
            "accent": "#FFB900",
            "icon_dark": "#FFFFFF",
            "icon_light": "#000000",
            "border": "#3F3F3F",
            "widget_background": "#2D2D2D",
            "report_background": "#2D2D2D",
            "report_text": "#FFFFFF",
            "report_description": "#A0A0A0",
        }

    def toggle_theme(self):
        self.dark_theme = not self.dark_theme
        if self.dark_theme:
            self.custom_colors.update(
                {
                    "background": "#1E1E1E",
                    "text": "#FFFFFF",
                    "widget_background": "#2D2D2D",
                    "report_background": "#2D2D2D",
                    "border": "#3F3F3F",
                    "report_text": "#FFFFFF",
                    "report_description": "#A0A0A0",
                }
            )
        else:
            self.custom_colors.update(
                {
                    "background": "#F0F0F0",
                    "text": "#000000",
                    "widget_background": "#FFFFFF",
                    "report_background": "#FFFFFF",
                    "border": "#CCCCCC",
                    "report_text": "#000000",
                    "report_description": "#505050",
                }
            )

    def get_stylesheet(self):
        return f"""
            QMainWindow, QWidget {{
                background-color: {self.custom_colors["background"]};
                color: {self.custom_colors["text"]};
            }}
            QPushButton {{
                background-color: {self.custom_colors["primary"]};
                color: {self.custom_colors["text"]};
                border: none;
                padding: 5px 10px;
                border-radius: 3px;
            }}
            QPushButton:hover {{
                background-color: {self.custom_colors["secondary"]};
            }}
            QTextEdit, QListWidget, QPlainTextEdit, QWidget#reportWidget {{
                border: 1px solid {self.custom_colors["border"]};
                background-color: {self.custom_colors["widget_background"]};
                color: {self.custom_colors["text"]};
                border-radius: 5px;
                padding: 5px;
            }}
            QLabel#sectionLabel {{
                color: {self.custom_colors["primary"]};
                font-size: 16px;
                font-weight: bold;
                margin-bottom: 10px;
            }}
            QLabel#reportValue {{
                color: {self.custom_colors["report_text"]};
                font-size: 24px;
                font-weight: bold;
            }}
            QLabel#reportDescription {{
                color: {self.custom_colors["report_description"]};
                font-size: 12px;
            }}
            QWidget#reportWidget {{
                background-color: {self.custom_colors["report_background"]};
            }}
            QTabWidget::pane {{
                border: 1px solid {self.custom_colors["border"]};
                border-radius: 5px;
            }}
            QTabBar::tab {{
                background-color: {self.custom_colors["widget_background"]};
                color: {self.custom_colors["text"]};
                padding: 5px 10px;
                border-top-left-radius: 3px;
                border-top-right-radius: 3px;
            }}
            QTabBar::tab:selected {{
                background-color: {self.custom_colors["primary"]};
                color: {self.custom_colors["text"]};
            }}
            QLineEdit {{
                border: 1px solid {self.custom_colors["border"]};
                background-color: {self.custom_colors["widget_background"]};
                color: {self.custom_colors["text"]};
                padding: 3px;
                border-radius: 3px;
            }}
            QMenuBar {{
                background-color: {self.custom_colors["background"]};
                color: {self.custom_colors["text"]};
            }}
            QMenuBar::item {{
                background-color: transparent;
            }}
            QMenuBar::item:selected {{
                background-color: {self.custom_colors["secondary"]};
            }}
            QMenuBar::item:pressed {{
                background-color: {self.custom_colors["primary"]};
            }}
            QMenu {{
                background-color: {self.custom_colors["background"]};
                color: {self.custom_colors["text"]};
                border: 1px solid {self.custom_colors["border"]};
            }}
            QMenu::item:selected {{
                background-color: {self.custom_colors["secondary"]};
            }}
        """

    def update_color(self, color_key, color_value):
        if color_key in self.custom_colors:
            self.custom_colors[color_key] = color_value


class AdvancedPatternBuilder(QWizard):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Advanced Pattern Builder")
        self.setWizardStyle(QWizard.ModernStyle)
        self.setMinimumSize(700, 500)
        self.addPage(self.createPatternPage())
        self.addPage(self.createPreviewPage())

    def createPatternPage(self):
        page = QWizardPage()
        page.setTitle("Define Patterns")
        layout = QVBoxLayout()
        self.pattern_table = QTableWidget()
        self.pattern_table.setColumnCount(4)
        self.pattern_table.setHorizontalHeaderLabels(
            ["Start Condition", "End Condition Type", "End Condition", "Number Length"]
        )
        self.pattern_table.horizontalHeader().setStretchLastSection(False)
        self.pattern_table.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        header = self.pattern_table.horizontalHeader()
        for i in range(4):
            header.setSectionResizeMode(i, QHeaderView.Stretch)
        layout.addWidget(self.pattern_table)
        add_button = QPushButton("Create Pattern")
        add_button.clicked.connect(self.addPattern)
        layout.addWidget(add_button)
        options_layout = QHBoxLayout()
        self.case_sensitive = QCheckBox("Case sensitive")
        options_layout.addWidget(self.case_sensitive)
        self.whole_words = QCheckBox("Match whole words only")
        options_layout.addWidget(self.whole_words)
        options_layout.addStretch()
        layout.addLayout(options_layout)
        page.setLayout(layout)
        return page

    def createPreviewPage(self):
        page = QWizardPage()
        page.setTitle("Preview and Test")
        layout = QVBoxLayout()
        self.pattern_preview = QPlainTextEdit()
        self.pattern_preview.setReadOnly(True)
        layout.addWidget(QLabel("Pattern Preview:"))
        layout.addWidget(self.pattern_preview)
        self.explanation = QLabel()
        self.explanation.setWordWrap(True)
        layout.addWidget(QLabel("Explanation:"))
        layout.addWidget(self.explanation)
        self.test_input = QTextEdit()
        self.test_input.setPlaceholderText("Enter test text here")
        layout.addWidget(QLabel("Test your pattern:"))
        layout.addWidget(self.test_input)
        self.test_button = QPushButton("Test Pattern")
        self.test_button.clicked.connect(self.testPattern)
        layout.addWidget(self.test_button)
        page.setLayout(layout)
        return page

    def addPattern(self):
        row_position = self.pattern_table.rowCount()
        self.pattern_table.insertRow(row_position)
        start_edit = QLineEdit()
        self.pattern_table.setCellWidget(row_position, 0, start_edit)
        end_type_combo = QComboBox()
        end_type_combo.addItems(["Single Number", "Multiple Numbers", "Specific Sequence"])
        self.pattern_table.setCellWidget(row_position, 1, end_type_combo)
        end_type_combo.currentIndexChanged.connect(
            lambda index, row=row_position: self.updateEndCondition(row, index)
        )
        end_edit = QLineEdit()
        self.pattern_table.setCellWidget(row_position, 2, end_edit)
        number_length_edit = QLineEdit()
        number_length_validator = QIntValidator(1, 100, self)
        number_length_edit.setValidator(number_length_validator)
        number_length_edit.setEnabled(False)
        self.pattern_table.setCellWidget(row_position, 3, number_length_edit)

    def updateEndCondition(self, row, index):
        end_edit = self.pattern_table.cellWidget(row, 2)
        number_length_edit = self.pattern_table.cellWidget(row, 3)
        if index == 0:
            end_edit.setEnabled(True)
            end_edit.setValidator(QIntValidator(0, 9, self))
            number_length_edit.setEnabled(False)
        elif index == 1:
            end_edit.setEnabled(False)
            number_length_edit.setEnabled(True)
        elif index == 2:
            end_edit.setEnabled(True)
            end_edit.setValidator(None)
            number_length_edit.setEnabled(False)

    def getPatternData(self):
        pattern_data = []
        for row in range(self.pattern_table.rowCount()):
            start = self.pattern_table.cellWidget(row, 0).text().strip()
            end_type = self.pattern_table.cellWidget(row, 1).currentText()
            end = self.pattern_table.cellWidget(row, 2).text().strip()
            number_length = self.pattern_table.cellWidget(row, 3).text().strip()
            if start and end:
                pattern_data.append(
                    {
                        "start": start,
                        "end_type": end_type,
                        "end": end,
                        "number_length": number_length,
                    }
                )
        return pattern_data

    def updatePattern(self):
        try:
            pattern_data = self.getPatternData()
            patterns = []
            for data in pattern_data:
                start = re.escape(data["start"])
                if data["end_type"] == "Single Number":
                    end = r"\d"
                    pattern = rf"{start}.*?{end}"
                elif data["end_type"] == "Multiple Numbers":
                    if not data["number_length"].isdigit():
                        raise ValueError(
                            "Number Length must be a positive integer for Multiple Numbers."
                        )
                    end = r"\d{" + data["number_length"] + "}"
                    pattern = rf"{start}.*?{end}"
                else:  # Specific Sequence
                    end = re.escape(data["end"])
                    pattern = rf"{start}.*?{end}"
                patterns.append(pattern)
            final_pattern = "|".join(patterns)
            if self.whole_words.isChecked():
                final_pattern = rf"\b({final_pattern})\b"
            flags = re.DOTALL | (0 if self.case_sensitive.isChecked() else re.IGNORECASE)
            self.final_pattern = re.compile(final_pattern, flags)
            self.pattern_preview.setPlainText(final_pattern)
            self.explanation.setText(f"This pattern will match: {', '.join(patterns)}")
            logging.debug(f"Final regex pattern: {final_pattern} with flags: {flags}")
        except re.error as e:
            ErrorHandler.show_error(
                f"The entered pattern is invalid:\n{str(e)}", "Invalid Pattern", self
            )
            logging.error(f"Invalid regex pattern: {str(e)}")
        except ValueError as ve:
            ErrorHandler.show_error(str(ve), "Invalid Input", self)
            logging.error(f"Invalid input for pattern: {str(ve)}")

    def testPattern(self):
        self.updatePattern()
        text = self.test_input.toPlainText()
        if not hasattr(self, "final_pattern") or not self.final_pattern:
            ErrorHandler.show_error(
                "Please define a valid pattern first.", "No Pattern Defined", self
            )
            return
        try:
            matches = list(self.final_pattern.finditer(text))
            cursor = self.test_input.textCursor()
            text_format = QTextCharFormat()
            text_format.setBackground(Qt.yellow)
            cursor.beginEditBlock()
            cursor.select(QTextCursor.Document)
            cursor.setCharFormat(QTextCharFormat())
            cursor.clearSelection()
            for match in matches:
                cursor.setPosition(match.start())
                cursor.setPosition(match.end(), QTextCursor.KeepAnchor)
                cursor.setCharFormat(text_format)
            cursor.endEditBlock()
            if not matches:
                ErrorHandler.show_warning(
                    "The pattern did not match any text in the sample.",
                    "No Matches Found",
                    self,
                )
            else:
                ErrorHandler.show_warning(f"Found {len(matches)} matches.", "Matches Found", self)
        except Exception as e:
            ErrorHandler.show_error(
                f"An error occurred while testing the pattern:\n{str(e)}",
                "Error Testing Pattern",
                self,
            )
            logging.error(f"Error testing pattern: {str(e)}")

    def getPattern(self):
        self.updatePattern()
        return self.final_pattern


class RegexHighlighter(QSyntaxHighlighter):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.formats = {
            "meta_char": QTextCharFormat(),
            "quantifier": QTextCharFormat(),
            "grouping": QTextCharFormat(),
            "character_class": QTextCharFormat(),
            "escaped_char": QTextCharFormat(),
            "literal": QTextCharFormat(),
        }
        self.formats["meta_char"].setForeground(QColor("#C586C0"))
        self.formats["quantifier"].setForeground(QColor("#D16969"))
        self.formats["grouping"].setForeground(QColor("#4EC9B0"))
        self.formats["character_class"].setForeground(QColor("#DCDCAA"))
        self.formats["escaped_char"].setForeground(QColor("#9CDCFE"))
        self.formats["literal"].setForeground(QColor("#FFFFFF"))

    def highlightBlock(self, text):
        index = 0
        while index < len(text):
            char = text[index]
            if char == "\\" and index + 1 < len(text):
                self.setFormat(index, 2, self.formats["escaped_char"])
                index += 2
                continue
            if char in ".^$|?*+(){}[]":
                if char in "(){}[]":
                    self.setFormat(index, 1, self.formats["grouping"])
                elif char in "?*+":
                    self.setFormat(index, 1, self.formats["quantifier"])
                else:
                    self.setFormat(index, 1, self.formats["meta_char"])
                index += 1
                continue
            self.setFormat(index, 1, self.formats["literal"])
            index += 1


class ParametersDialog(QDialog):
    def __init__(self, current_parameters, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Processing Parameters")

        # Set optimal dimensions
        self.setMinimumSize(450, 300)  # Wider to show tabs, shorter height
        self.setMaximumSize(600, 500)  # Prevent dialog from getting too large

        # Create main layout with reduced margins
        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 10, 10)  # Reduce outer margins
        layout.setSpacing(5)  # Reduce spacing between elements

        # Initialize parameters
        self.parameters = current_parameters.copy()
        self.pattern_data = self.parameters.get("pattern_data", [])

        # Descriptions for tooltips
        self.descriptions = {
            # Basic Cleanup
            "remove_page_numbers": "Delete standalone page numbers that are isolated on their own lines within the text.",
            "remove_roman_page_numbers": "Remove standalone Roman numeral page numbers that appear on their own lines.",
            "remove_page_indicators": "Remove occurrences of page indicators like 'Page xvi', 'pag. xvi', considering different capitalizations and abbreviations.",
            "remove_page_delimiters": "Remove page delimiter markers (e.g., '--- Page 123 ---') that are created by the PDF converter.",
            "normalize_spacing": "Adjust spacing to ensure consistent formatting throughout the text.",
            "remove_break_lines": "Join all line breaks into a single continuous text.",
            "normalize_line_breaks": "Fix irregular line breaks and remove scattered characters from PDF-to-text conversions.",
            # Text Transformation
            "lowercase": "Convert all text to lowercase characters.",
            "normalize_unicode": "Standardize text encoding to ensure consistency.",
            "remove_diacritics": "Replace diacritical marks (accents) from characters (e.g., 'João' becomes 'Joao').",
            "word_tokenization": "Split the text into individual words (tokens) for analysis.",
            "remove_stop_words": "Eliminate common words (prepositions, articles, etc.) that typically don't carry significant meaning.",
            # Character Sets
            "remove_greek": "Remove all Greek alphabet characters from the text.",
            "remove_cyrillic": "Remove all Cyrillic alphabet characters from the text.",
            "remove_super_sub_script": "Remove superscript and subscript characters.",
            "strip_html": "Remove any HTML markup tags from the text.",
            # Advanced
            "remove_bibliographical_references": "Remove citations like (Author 1994) from the text.",
        }

        # Create Tabs
        tabs = QTabWidget()

        # Basic Cleanup Tab
        basic_tab = QWidget()
        basic_layout = QVBoxLayout(basic_tab)
        basic_options = [
            ("remove_page_delimiters", "Remove Page Delimiters (--- Page X ---)"),
            ("remove_page_numbers", "Remove Page Numbers"),
            ("remove_roman_page_numbers", "Remove Roman Numeral Page Numbers"),
            ("remove_page_indicators", "Remove Page Indicators (e.g., 'Page xvi')"),
            ("normalize_spacing", "Normalize Spacing"),
            ("remove_break_lines", "Join Break Lines"),
            ("normalize_line_breaks", "Normalize Line Breaks"),
        ]

        self.add_options_to_layout(basic_layout, basic_options)
        tabs.addTab(basic_tab, "Basic Cleanup")

        # Text Transformation Tab
        transform_tab = QWidget()
        transform_layout = QVBoxLayout(transform_tab)
        transform_options = [
            ("lowercase", "Apply Lowercase"),
            ("normalize_unicode", "Normalize Unicode"),
            ("remove_diacritics", "Diacritic Replacement"),
            ("word_tokenization", "Word Tokenization"),
            ("remove_stop_words", "Remove Stop Words"),
        ]
        self.add_options_to_layout(transform_layout, transform_options)
        tabs.addTab(transform_tab, "Text Transformation")

        # Character Sets Tab
        charset_tab = QWidget()
        charset_layout = QVBoxLayout(charset_tab)
        charset_options = [
            ("remove_greek", "Remove Greek letters"),
            ("remove_cyrillic", "Remove Cyrillic Script"),
            ("remove_super_sub_script", "Remove Superscript and Subscript Characters"),
            ("strip_html", "Strip HTML tags"),
        ]
        self.add_options_to_layout(charset_layout, charset_options)
        tabs.addTab(charset_tab, "Character Sets")

        # Advanced Tab
        advanced_tab = QWidget()
        advanced_layout = QVBoxLayout(advanced_tab)

        # Regex Pattern Builder
        regex_button = QPushButton("Define Patterns with Regex")
        regex_button.clicked.connect(self.open_regex_dialog)
        advanced_layout.addWidget(regex_button)

        self.regex_display = QPlainTextEdit()
        self.regex_display.setReadOnly(True)
        self.regex_display.setPlainText(self.parameters.get("regex_pattern") or "None")
        self.regex_display.setFont(QFont("Courier New", 10))
        self.regex_highlighter = RegexHighlighter(self.regex_display.document())

        advanced_layout.addWidget(QLabel("Current pattern:"))
        advanced_layout.addWidget(self.regex_display)

        # Character Selection Button
        char_remove_button = QPushButton("Select Characters or Sequences to Remove")
        char_remove_button.clicked.connect(self.open_char_selection)
        advanced_layout.addWidget(char_remove_button)

        # **Define self.char_list instead of self.char_list_widget**
        self.char_list = QListWidget()
        self.update_char_list()  # No arguments needed now
        advanced_layout.addWidget(QLabel("Selected items:"))
        advanced_layout.addWidget(self.char_list)

        # Additional Advanced Options
        advanced_options = [
            ("remove_bibliographical_references", "Remove Bibliographical References")
        ]
        self.add_options_to_layout(advanced_layout, advanced_options)

        tabs.addTab(advanced_tab, "Advanced")

        layout.addWidget(tabs)

        # Dialog Buttons
        buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

        # Initialize Checkboxes based on parameters
        self.update_checkboxes()

    def add_options_to_layout(self, layout, options):
        for key, label in options:
            h_layout = QHBoxLayout()

            checkbox = QCheckBox(label)
            checkbox.setChecked(self.parameters.get(key, False))
            checkbox.stateChanged.connect(
                lambda state, k=key: self.parameters.update({k: bool(state)})
            )

            if key in self.descriptions:
                checkbox.setToolTip(self.descriptions[key])

                info_button = QToolButton()
                info_button.setIcon(self.style().standardIcon(QStyle.SP_MessageBoxInformation))
                info_button.setToolTip(self.descriptions[key])
                info_button.setFixedSize(16, 16)
                info_button.setStyleSheet(
                    "QToolButton { border: none; background-color: transparent; }"
                )

                h_layout.addWidget(checkbox)
                h_layout.addWidget(info_button)
                h_layout.addStretch()
            else:
                h_layout.addWidget(checkbox)
                h_layout.addStretch()

            layout.addLayout(h_layout)

        layout.addStretch()

    def open_regex_dialog(self):
        dialog = AdvancedPatternBuilder(self)
        if self.pattern_data:
            for row_data in self.pattern_data:
                dialog.addPattern()
                row_position = dialog.pattern_table.rowCount() - 1
                dialog.pattern_table.cellWidget(row_position, 0).setText(row_data["start"])
                dialog.pattern_table.cellWidget(row_position, 1).setCurrentText(
                    row_data["end_type"]
                )
                dialog.pattern_table.cellWidget(row_position, 2).setText(row_data["end"])
                dialog.pattern_table.cellWidget(row_position, 3).setText(
                    row_data["number_length"]
                )
        if dialog.exec():
            pattern = dialog.getPattern()
            if pattern:
                try:
                    re.compile(pattern.pattern)
                    self.parameters["regex_pattern"] = pattern.pattern
                    self.regex_display.setPlainText(pattern.pattern)
                    self.pattern_data = dialog.getPatternData()
                    self.parameters["pattern_data"] = self.pattern_data
                except re.error as e:
                    ErrorHandler.show_error(
                        f"The entered pattern is invalid:\n{str(e)}",
                        "Invalid Pattern",
                        self,
                    )
                    logging.error(f"Invalid regex pattern: {str(e)}")

    def open_char_selection(self):
        dialog = CharacterSelectionDialog(self.parameters.get("chars_to_remove", []), self)
        if dialog.exec():
            self.parameters["chars_to_remove"] = dialog.get_selected_chars()
            self.update_char_list()

    def update_char_list(self):
        # Fetch the latest characters or sequences to remove
        chars_to_remove = self.parameters.get("chars_to_remove", [])
        self.char_list.clear()
        for item in chars_to_remove:
            list_item = QListWidgetItem(item)
            self.char_list.addItem(list_item)

    def update_checkboxes(self):
        for checkbox in self.findChildren(QCheckBox):
            key = next(
                (
                    k
                    for k, v in [
                        ("remove_page_numbers", "Remove Page Numbers"),
                        (
                            "remove_roman_page_numbers",
                            "Remove Roman Numeral Page Numbers",
                        ),
                        (
                            "remove_page_indicators",
                            "Remove Page Indicators (e.g., 'Page xvi')",
                        ),
                        (
                            "remove_page_delimiters",
                            "Remove Page Delimiters (--- Page X ---)",
                        ),
                        (
                            "remove_bibliographical_references",
                            "Remove Bibliographical References",
                        ),
                        ("remove_greek", "Remove Greek letters"),
                        ("remove_cyrillic", "Remove Cyrillic Script"),
                        (
                            "remove_super_sub_script",
                            "Remove Superscript and Subscript Characters",
                        ),
                        ("remove_diacritics", "Diacritic Replacement"),
                        ("strip_html", "Strip HTML tags"),
                        ("lowercase", "Apply Lowercase"),
                        ("normalize_spacing", "Normalize Spacing"),
                        ("remove_stop_words", "Remove Stop Words"),
                        ("word_tokenization", "Word Tokenization"),
                        ("normalize_unicode", "Normalize Unicode"),
                        ("normalize_line_breaks", "Normalize Line Breaks"),
                        ("remove_break_lines", "Join Break Lines"),
                    ]
                    if v == checkbox.text()
                ),
                None,
            )
            if key:
                checkbox.setChecked(self.parameters.get(key, False))

    def get_parameters(self):
        return self.parameters


class CharacterSelectionDialog(QDialog):
    def __init__(self, current_chars, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Select Characters or Sequences to Remove")
        self.setMinimumSize(400, 300)
        layout = QVBoxLayout(self)
        self.selected_chars = list(current_chars)
        input_layout = QHBoxLayout()
        self.char_input = QLineEdit()
        self.char_input.setPlaceholderText("Enter characters or sequences to remove")
        input_layout.addWidget(self.char_input)
        include_button = QPushButton("Include")
        include_button.clicked.connect(self.add_chars)
        include_button.setDefault(True)
        input_layout.addWidget(include_button)
        layout.addLayout(input_layout)
        self.char_list = QListWidget()
        self.update_char_list(self.selected_chars)  # Correct attribute
        layout.addWidget(QLabel("Items to remove:"))
        layout.addWidget(self.char_list)
        delete_button = QPushButton("Delete Selected")
        delete_button.clicked.connect(self.delete_selected)
        layout.addWidget(delete_button)
        button_box = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        layout.addWidget(button_box)
        button_box.accepted.connect(self.accept)
        button_box.rejected.connect(self.reject)
        ok_button = button_box.button(QDialogButtonBox.Ok)
        if ok_button:
            ok_button.setAutoDefault(False)
            ok_button.setDefault(False)
        cancel_button = button_box.button(QDialogButtonBox.Cancel)
        if cancel_button:
            cancel_button.setAutoDefault(False)
            cancel_button.setDefault(False)
        self.char_input.returnPressed.connect(self.add_chars)

    def add_chars(self):
        new_item = self.char_input.text().strip()
        if new_item and new_item not in self.selected_chars:
            self.selected_chars.append(new_item)
            self.update_char_list(self.selected_chars)  # Use correct attribute
        self.char_input.clear()

    def update_char_list(self, chars_to_remove):
        self.char_list.clear()  # Correct attribute name
        for item in chars_to_remove:
            list_item = QListWidgetItem(item)
            self.char_list.addItem(list_item)

    def delete_selected(self):
        for item in self.char_list.selectedItems():
            self.selected_chars.remove(item.text())
        self.update_char_list(self.selected_chars)

    def get_selected_chars(self):
        return self.selected_chars


class FileLoadingDialog(QDialog):
    cancelled = Signal()

    def __init__(self, parent=None, title="Loading Files..."):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setModal(True)
        layout = QVBoxLayout(self)
        self.progress_bar = QProgressBar()
        self.progress_bar.setRange(0, 0)
        layout.addWidget(self.progress_bar)
        self.label = QLabel("Loading files...")
        layout.addWidget(self.label)
        self.cancel_button = QPushButton("Cancel")
        self.cancel_button.clicked.connect(self.on_cancel)
        layout.addWidget(self.cancel_button)

    def update_progress(self, current, total, time_remaining, error):
        if total > 0:
            self.progress_bar.setRange(0, total)
            self.progress_bar.setValue(current)
        if time_remaining is not None:
            self.label.setText(
                f"Processing file {current} of {total}... Estimated time remaining: {time_remaining:.2f} seconds"
            )
        else:
            self.label.setText(f"Processing file {current} of {total}...")
        if error:
            pass

    def on_cancel(self):
        self.cancelled.emit()
        self.reject()


class DirectoryLoadingWorker(QObject):
    finished = Signal(list)  # new_documents
    update_progress = Signal(int, int, float, str)  # current, total, time_remaining, error

    def __init__(self, file_manager, directory, signals):
        super().__init__()
        self.file_manager = file_manager
        self.directory = directory
        self.signals = signals
        self._is_cancelled = False

    @Slot()
    def run(self):
        try:
            start_time = time.time()
            new_documents = self.file_manager.add_directory(
                self.directory, self.signals, self.is_cancelled
            )
            elapsed = time.time() - start_time
            logging.info(
                f"Directory scan/load start and finish: {len(new_documents)} files in {elapsed:.2f}s"
            )
            if not self._is_cancelled:
                self.signals.processing_complete.emit(
                    new_documents, []
                )  # Assuming no warnings here
            self.finished.emit(new_documents)
        except Exception as e:
            logging.error(f"Error loading directory {self.directory}: {e}")
            self.signals.loading_error.emit(str(e))
            self.finished.emit([])

    def cancel(self):
        self._is_cancelled = True

    def is_cancelled(self):
        return self._is_cancelled


class DocumentLoaderWorker(QObject):
    finished = Signal(int, str, str, str)  # token, path, original, processed
    error = Signal(int, str, str)  # token, path, message

    def __init__(self, file_path, processed_text, token):
        super().__init__()
        self.file_path = file_path
        self.processed_text = processed_text
        self.token = token
        self._cancelled = False

    def cancel(self):
        self._cancelled = True

    @Slot()
    def run(self):
        if self._cancelled:
            return
        try:
            original_text = load_extracted_text(self.file_path)
            if self._cancelled:
                return
            processed_text = (
                self.processed_text if self.processed_text is not None else original_text
            )
            self.finished.emit(
                self.token,
                self.file_path,
                original_text,
                processed_text,
            )
        except Exception as exc:
            if not self._cancelled:
                self.error.emit(self.token, self.file_path, str(exc))


class CorpusPreviewWorker(QObject):
    finished = Signal(int, str, str)  # token, original_preview, processed_preview
    error = Signal(int, str)  # token, message

    def __init__(self, documents, token, max_chars, include_paths, is_processed):
        super().__init__()
        self.documents = documents
        self.token = token
        self.max_chars = max_chars
        self.include_paths = include_paths
        self.is_processed = is_processed
        self._cancelled = False

    def cancel(self):
        self._cancelled = True

    @Slot()
    def run(self):
        if self._cancelled:
            return
        try:
            start_time = time.time()
            parts = []
            total = len(self.documents)
            for i, doc in enumerate(self.documents):
                if self._cancelled:
                    return
                header = f"\nFILE {i + 1:03d} / {total}\n" if total > 1 else ""
                if self.include_paths and header:
                    header += f"{doc.file_path}\n"
                elif total == 1 and self.include_paths:
                    header = f"{doc.file_path}\n\n"

                if self.is_processed:
                    preview, trunc = doc.get_processed_preview(self.max_chars)
                else:
                    preview, trunc = doc.get_original_preview(self.max_chars)

                if trunc:
                    preview += "\n[...TRUNCATED...]"

                parts.append(header + preview)

            elapsed = time.time() - start_time
            mode = "Preview Processed" if self.is_processed else "Preview Original"
            logging.info(
                f"Preview generated: mode={mode}, files={total}, chars={self.max_chars}, elapsed={elapsed:.2f}s"
            )

            result = "".join(parts).strip()
            if self.is_processed:
                self.finished.emit(self.token, "", result)
            else:
                self.finished.emit(self.token, result, "")
        except Exception as exc:
            if not self._cancelled:
                self.error.emit(self.token, str(exc))


class ReportWorker(QObject):
    progress = Signal(int)
    finished = Signal(dict)

    def __init__(self, files, parameters, processed=False, processed_results=None):
        super().__init__()
        self.files = files
        self.parameters = parameters or {}
        self.processed = processed
        self.processed_results = processed_results or []
        self.batch_size = 100
        self.chunk_size = 100000  # Process text in chunks of 100,000 characters
        self.use_rust = rust_generate_report_summary is not None
        self._processed_lookup = (
            {path: processed_text for path, _, processed_text in self.processed_results}
            if self.processed
            else {}
        )
        self.nlp = None if self.use_rust else get_spacy_model()

    def run(self):
        try:
            if self.use_rust:
                self._run_with_rust()
            else:
                self._run_with_python()
        except Exception as e:
            logging.error(f"Error in report generation: {str(e)}")

    def _run_with_rust(self):
        if rust_generate_report_summary is None:
            raise RuntimeError("Rust report summary binding unavailable")

        processed_lookup = self._processed_lookup if self.processed else {}
        lookup_arg = processed_lookup or None

        def progress_callback(value):
            try:
                self.progress.emit(int(value))
            except Exception:
                logging.exception("Failed to emit report progress update from Rust")

        report_data = rust_generate_report_summary(
            self.files,
            self.processed,
            bool(self.parameters.get("word_tokenization", False)),
            processed_lookup=lookup_arg,
            progress_callback=progress_callback,
        )

        if report_data is None:
            report_data = {}
        elif not isinstance(report_data, dict):
            report_data = dict(report_data)

        self.progress.emit(100)
        self.finished.emit(report_data)

    def _run_with_python(self):
        total_words = 0
        total_size = 0
        file_count = len(self.files)

        if file_count == 0:
            self.progress.emit(100)

        for i in range(0, file_count, self.batch_size):
            batch = self.files[i : i + self.batch_size]
            batch_words, batch_size = self.process_batch(batch)

            total_words += batch_words
            total_size += batch_size

            if file_count:
                progress = min(100, int((i + len(batch)) / file_count * 100))
                self.progress.emit(progress)

        avg_words = total_words / file_count if file_count else 0
        avg_size = total_size / file_count if file_count else 0

        final_report = {
            "total_files": file_count,
            "total_size": total_size / (1024 * 1024),
            "avg_size": avg_size / (1024 * 1024),
            "total_words": total_words,
            "avg_words": avg_words,
        }

        self.progress.emit(100)
        self.finished.emit(final_report)

    def process_batch(self, batch):
        batch_words = 0
        batch_size = 0
        for file_path in batch:
            if self.processed:
                text = self._processed_lookup.get(file_path, "")
            else:
                text = extract_document(file_path).text

            batch_size += len(text.encode("utf-8"))

            if self.parameters.get("word_tokenization"):
                tokens = text.split()
                batch_words += len(tokens)
            else:
                for i in range(0, len(text), self.chunk_size):
                    chunk = text[i : i + self.chunk_size]
                    doc = self.nlp(chunk)
                    batch_words += len([token for token in doc if not token.is_space])

        return batch_words, batch_size


class PreprocessorGUI(QMainWindow):
    def __init__(self):
        super().__init__()
        self.version = "0.8"
        self.file_manager = FileManager()
        self.theme_manager = ThemeManager()
        self.processor = DocumentProcessor()
        self.signals = AppSignals()
        self.base_path = os.path.dirname(os.path.abspath(__file__))
        self.assets_path = os.path.join(self.base_path, "assets")
        self.icon_path = resource_path(os.path.join("assets", "my_icon.ico"))
        self.font_path = resource_path(os.path.join("assets", "fonts", "Roboto-Regular.ttf"))
        self.current_file = None
        self.corpus_name = "Untitled Corpus"
        self.thread_pool = QThreadPool()
        self.thread_pool.setMaxThreadCount(multiprocessing.cpu_count())
        self.processed_results = []
        self.errors = []
        self.warnings = []
        self.active_workers = 0
        self.files_processed = 0
        self.total_size = 0
        self.total_time = 0
        self.processed_size = 0
        self.documentation_window = None
        self.report_thread = QThread()
        self.report_worker = None
        self.lock = Lock()
        self.previous_search_term = ""
        self.previous_text_edit = None
        self.document_loader_thread = None
        self.document_loader_worker = None
        self.document_request_token = 0
        self._active_loading_token = None
        self._busy_cursor_depth = 0
        self.init_ui()

    def init_ui(self):
        logging.info("Starting init_ui")
        self.setFocusPolicy(Qt.StrongFocus)
        self.setWindowTitle("CorpusAid")
        self.setGeometry(100, 100, 1200, 800)
        self.setFont(QFont("Roboto", 10))
        icon_path = resource_path("assets/my_icon.ico")
        if os.path.exists(icon_path):
            self.setWindowIcon(QIcon(icon_path))
        self.create_menu_bar()
        self.create_toolbar()
        self.setup_central_widget()
        self.setup_status_bar()
        self.search_input.returnPressed.connect(self.search_text)
        self.prev_button.clicked.connect(self.go_to_previous_occurrence)
        self.next_button.clicked.connect(self.go_to_next_occurrence)
        self.text_tabs.currentChanged.connect(self.on_tab_changed)
        self.processed_text.customContextMenuRequested.connect(
            self.show_processed_text_context_menu
        )
        self.signals.update_progress.connect(self.update_progress)
        self.signals.result.connect(self.handle_result)
        self.signals.error.connect(self.handle_error)
        self.signals.warning.connect(self.handle_warning)
        self.signals.finished.connect(self.on_all_workers_finished)
        # self.signals.processing_complete.connect(self.display_report)
        self.apply_theme()
        self.showMaximized()
        QTimer.singleShot(1000, lambda: self.check_for_updates(manual_trigger=False))

    def _set_busy_cursor(self, enable):
        if enable:
            self._busy_cursor_depth += 1
            if self._busy_cursor_depth == 1:
                QApplication.setOverrideCursor(Qt.WaitCursor)
        else:
            if self._busy_cursor_depth > 0:
                self._busy_cursor_depth -= 1
                if self._busy_cursor_depth == 0:
                    QApplication.restoreOverrideCursor()

    def _force_hide_loading_indicator(self):
        if self.preview_loading_widget.isVisible():
            self.preview_loading_widget.hide()
        self.preview_loading_label.clear()
        self.preview_loading_bar.setRange(0, 1)
        self.preview_loading_bar.setValue(0)
        self.text_tabs.setEnabled(True)
        if self._busy_cursor_depth > 0:
            QApplication.restoreOverrideCursor()
            self._busy_cursor_depth = 0
        self._active_loading_token = None

    def _show_loading_indicator(self, source, token, message, determinate=False):
        self._force_hide_loading_indicator()
        self._active_loading_token = (source, token)
        self.preview_loading_label.setText(message)
        if determinate:
            self.preview_loading_bar.setRange(0, 100)
            self.preview_loading_bar.setValue(0)
        else:
            self.preview_loading_bar.setRange(0, 0)
        self.preview_loading_widget.show()
        self.text_tabs.setEnabled(False)
        self._set_busy_cursor(True)

    def _update_loading_indicator(
        self, source, token, value=None, message=None, determinate=None
    ):
        if self._active_loading_token != (source, token):
            return
        if determinate is not None:
            if determinate:
                if self.preview_loading_bar.maximum() == 0:
                    self.preview_loading_bar.setRange(0, 100)
                    self.preview_loading_bar.setValue(0)
            else:
                self.preview_loading_bar.setRange(0, 0)
        if message is not None:
            self.preview_loading_label.setText(message)
        if value is not None and self.preview_loading_bar.maximum() != 0:
            self.preview_loading_bar.setValue(max(0, min(100, value)))

    def _hide_loading_indicator(self, source, token):
        if self._active_loading_token != (source, token):
            return
        self._set_busy_cursor(False)
        self._force_hide_loading_indicator()

    def resource_path(self, relative_path):
        """Get absolute path to resource, works for dev and for PyInstaller"""
        try:
            # PyInstaller creates a temp folder and stores path in _MEIPASS
            base_path = sys._MEIPASS
        except Exception:
            base_path = os.path.abspath(".")
        return os.path.join(base_path, relative_path)

    def create_menu_bar(self):
        menu_bar = self.menuBar()
        # File Menu
        file_menu = menu_bar.addMenu("&File")
        self.new_action = self.create_action(
            "New",
            "document-new",
            "Ctrl+N",
            "Start a new project",
            self.confirm_start_new_cleaning,
        )
        file_menu.addAction(self.new_action)
        self.open_files_action = self.create_action(
            "Open Files", "document-open", "Ctrl+O", "Open files", self.open_file
        )
        file_menu.addAction(self.open_files_action)
        self.open_directory_action = self.create_action(
            "Open Directory",
            "folder-open",
            "Ctrl+Shift+O",
            "Open directory",
            self.open_directory,
        )
        file_menu.addAction(self.open_directory_action)
        self.save_action = self.create_action(
            "Save", "document-save", "Ctrl+S", "Save current file", self.save_file
        )
        file_menu.addAction(self.save_action)
        self.export_processed_corpus_action = self.create_action(
            "Export Processed Corpus...",
            "document-export",
            "Ctrl+Shift+E",
            "Export processed corpus texts and manifests",
            self.export_processed_corpus,
        )
        file_menu.addAction(self.export_processed_corpus_action)
        file_menu.addSeparator()
        self.exit_action = self.create_action(
            "Exit", "application-exit", "Ctrl+Q", "Exit the application", self.close
        )
        file_menu.addAction(self.exit_action)

        # Edit Menu
        edit_menu = menu_bar.addMenu("&Edit")
        self.undo_action = self.create_action(
            "Undo", "edit-undo", "Ctrl+Z", "Undo last action", self.undo
        )
        edit_menu.addAction(self.undo_action)
        self.redo_action = self.create_action(
            "Redo", "edit-redo", "Ctrl+Y", "Redo last action", self.redo
        )
        edit_menu.addAction(self.redo_action)

        # Settings Menu
        settings_menu = menu_bar.addMenu("&Settings")
        self.toggle_theme_action = self.create_action(
            "Toggle Theme",
            "preferences-desktop-theme",
            "",
            "Switch between light and dark theme",
            self.toggle_theme,
        )
        settings_menu.addAction(self.toggle_theme_action)
        self.processing_parameters_action = self.create_action(
            "Processing Parameters",
            "preferences-system",
            "",
            "Configure processing options",
            self.open_parameters_dialog,
        )
        settings_menu.addAction(self.processing_parameters_action)

        # Help Menu
        help_menu = menu_bar.addMenu("&Help")
        self.about_action = self.create_action(
            "About", "help-about", "", "About this application", self.show_about_dialog
        )
        help_menu.addAction(self.about_action)
        self.documentation_action = self.create_action(
            "Documentation",
            "help-contents",
            "F1",
            "View documentation",
            self.show_documentation,
        )
        help_menu.addAction(self.documentation_action)
        self.check_updates_action = self.create_action(
            "Check for Updates",
            "system-software-update",
            "",
            "Check for updates",
            lambda: self.check_for_updates(manual_trigger=True),
        )
        help_menu.addAction(self.check_updates_action)

    def create_toolbar(self):
        self.toolbar = QToolBar()
        self.addToolBar(self.toolbar)
        self.toolbar.addAction(self.new_action)
        self.toolbar.addAction(self.open_files_action)
        self.toolbar.addAction(self.open_directory_action)
        self.toolbar.addAction(self.save_action)
        self.toolbar.addAction(self.export_processed_corpus_action)

        self.process_corpus_action = QAction(
            QIcon.fromTheme("system-run"), "Process Loaded Corpus", self
        )
        self.process_corpus_action.triggered.connect(self.process_files)
        self.toolbar.addAction(self.process_corpus_action)

    def create_action(self, text, icon, shortcut, tooltip, callback):
        action = QAction(QIcon.fromTheme(icon, QIcon()), text, self)
        if shortcut:
            if sys.platform == "darwin":
                shortcut = shortcut.replace("Ctrl", "Meta")
            action.setShortcut(QKeySequence(shortcut))
            action.setShortcutContext(Qt.ApplicationShortcut)
        action.setToolTip(tooltip)
        action.triggered.connect(callback)
        return action

    def setup_central_widget(self):
        central_widget = QWidget()
        main_layout = QHBoxLayout(central_widget)
        main_layout.setContentsMargins(0, 0, 0, 0)
        main_layout.setSpacing(0)

        # Left Panel
        left_panel = QWidget()
        left_layout = QVBoxLayout(left_panel)
        left_panel.setMinimumWidth(350)  # Set minimum width for visibility

        # File List Widget
        file_list_widget = QWidget()
        file_list_layout = QVBoxLayout(file_list_widget)
        selected_files_label = QLabel("Selected Files:")
        selected_files_label.setObjectName("sectionLabel")
        file_list_layout.addWidget(selected_files_label)

        self.file_list = FileListWidget()
        self.file_list.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.file_list.files_added.connect(lambda docs: self.update_report())
        self.file_list.files_removed.connect(lambda paths: self.file_manager.remove_files(paths))
        self.file_list.itemSelectionChanged.connect(self.refresh_display)
        file_list_layout.addWidget(self.file_list)
        left_layout.addWidget(file_list_widget)

        # Report Widget
        report_widget = self.create_report_area()
        left_layout.addWidget(report_widget)

        left_layout.setStretch(0, 2)  # file_list_widget
        left_layout.setStretch(1, 1)  # report_widget

        # Text Display
        text_display = QWidget()
        text_layout = QVBoxLayout(text_display)
        text_display.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)

        self.preview_loading_widget = QWidget()
        preview_loading_layout = QHBoxLayout(self.preview_loading_widget)
        preview_loading_layout.setContentsMargins(0, 0, 0, 0)
        preview_loading_layout.setSpacing(8)
        self.preview_loading_label = QLabel()
        self.preview_loading_label.setObjectName("previewLoadingLabel")
        self.preview_loading_bar = QProgressBar()
        self.preview_loading_bar.setRange(0, 0)
        preview_loading_layout.addWidget(self.preview_loading_label)
        preview_loading_layout.addWidget(self.preview_loading_bar, 1)
        self.preview_loading_widget.hide()

        # Preview Control Panel
        preview_panel = QWidget()
        preview_layout = QHBoxLayout(preview_panel)
        preview_layout.setContentsMargins(0, 0, 0, 0)
        preview_layout.setSpacing(10)

        self.preview_mode_combo = QComboBox()
        self.preview_mode_combo.addItems(["Selected file", "Selected files", "First N files"])
        self.preview_mode_combo.currentTextChanged.connect(self._toggle_preview_controls)

        self.preview_count_spin = QSpinBox()
        self.preview_count_spin.setRange(1, 1000)
        self.preview_count_spin.setValue(50)
        self.preview_count_spin.setPrefix("Files: ")

        self.preview_chars_spin = QSpinBox()
        self.preview_chars_spin.setRange(100, 50000)
        self.preview_chars_spin.setSingleStep(100)
        self.preview_chars_spin.setValue(2000)
        self.preview_chars_spin.setPrefix("Max chars: ")

        self.preview_paths_check = QCheckBox("Paths")
        self.preview_paths_check.setChecked(True)

        self.preview_original_btn = QPushButton("Preview Original")
        self.preview_original_btn.clicked.connect(self.preview_original_action)

        self.preview_processed_btn = QPushButton("Preview Processed")
        self.preview_processed_btn.clicked.connect(self.preview_processed_action)

        preview_layout.addWidget(QLabel("Preview:"))
        preview_layout.addWidget(self.preview_mode_combo)
        preview_layout.addWidget(self.preview_count_spin)
        preview_layout.addWidget(self.preview_chars_spin)
        preview_layout.addWidget(self.preview_paths_check)
        preview_layout.addStretch()
        preview_layout.addWidget(self.preview_original_btn)
        preview_layout.addWidget(self.preview_processed_btn)

        self._toggle_preview_controls(self.preview_mode_combo.currentText())

        text_layout.addWidget(preview_panel)

        self.text_tabs = QTabWidget()
        self.original_text = QPlainTextEdit()
        self.original_text.setReadOnly(True)
        self.processed_text = QPlainTextEdit()
        self.processed_text.setReadOnly(True)

        # **Insert Here**
        self.processed_text.setContextMenuPolicy(Qt.CustomContextMenu)
        self.processed_text.customContextMenuRequested.connect(
            self.show_processed_text_context_menu
        )

        self.text_tabs.addTab(self.original_text, "Original Text")
        self.text_tabs.addTab(self.processed_text, "Processed Text")
        self.text_tabs.currentChanged.connect(self.on_tab_changed)

        text_layout.addWidget(self.preview_loading_widget)
        text_layout.addWidget(self.text_tabs)

        # Search Layout
        search_layout = QHBoxLayout()
        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Search in text...")
        self.search_input.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Fixed)
        search_layout.addWidget(self.search_input)

        self.prev_button = QPushButton()
        self.prev_button.setIcon(QIcon.fromTheme("go-previous"))
        self.prev_button.setToolTip("Previous occurrence")
        self.prev_button.clicked.connect(self.go_to_previous_occurrence)
        self.prev_button.setSizePolicy(QSizePolicy.Fixed, QSizePolicy.Fixed)
        search_layout.addWidget(self.prev_button)

        self.next_button = QPushButton()
        self.next_button.setIcon(QIcon.fromTheme("go-next"))
        self.next_button.setToolTip("Next occurrence")
        self.next_button.clicked.connect(self.go_to_next_occurrence)
        self.next_button.setSizePolicy(QSizePolicy.Fixed, QSizePolicy.Fixed)
        search_layout.addWidget(self.next_button)

        self.occurrence_label = QLabel("0/0")
        self.occurrence_label.setSizePolicy(QSizePolicy.Fixed, QSizePolicy.Fixed)
        search_layout.addWidget(self.occurrence_label)

        text_layout.addLayout(search_layout)

        self.current_occurrence_index = -1
        self.search_results = []

        # Splitter to allow resizable panels
        splitter = QSplitter(Qt.Horizontal)
        splitter.addWidget(left_panel)
        splitter.addWidget(text_display)
        splitter.setStretchFactor(0, 1)  # Left panel
        splitter.setStretchFactor(1, 2)  # Right panel (text display) gets more space

        main_layout.addWidget(splitter)

        self.setCentralWidget(central_widget)

    def create_report_area(self):
        outer_widget = QWidget()
        outer_layout = QVBoxLayout(outer_widget)

        report_label = QLabel("Summary Report:")
        report_label.setObjectName("sectionLabel")
        outer_layout.addWidget(report_label)

        report_widget = QWidget()
        report_widget.setObjectName("reportWidget")
        report_layout = QVBoxLayout(report_widget)

        self.summary_stack = QStackedWidget()

        # Empty Widget
        empty_widget = QWidget()
        self.summary_stack.addWidget(empty_widget)

        # Loading Widget
        loading_widget = QWidget()
        loading_layout = QVBoxLayout(loading_widget)
        self.loading_label = QLabel("Generating report...")
        self.loading_label.setAlignment(Qt.AlignCenter)
        loading_layout.addWidget(self.loading_label)
        self.report_progress_bar = QProgressBar()
        self.report_progress_bar.setRange(0, 100)
        self.report_progress_bar.setValue(0)
        loading_layout.addWidget(self.report_progress_bar)
        self.summary_stack.addWidget(loading_widget)

        # Report Content Widget
        self.report_grid = QGridLayout()
        self.report_grid.setSpacing(10)

        self.report_items = {
            "total_files": self.create_report_item("Total Files Processed", "0"),
            "total_size": self.create_report_item("Total Size of Processed Files", "0 MB"),
            "avg_size": self.create_report_item("Average File Size", "0 MB"),
            "total_words": self.create_report_item("Total Word Count", "0"),
            "avg_words": self.create_report_item("Average Word Count per File", "0"),
        }

        positions = [(i, j) for i in range(3) for j in range(2)]
        for (key, widget), position in zip(self.report_items.items(), positions):
            self.report_grid.addWidget(widget, *position)

        report_content = QWidget()
        report_content.setLayout(self.report_grid)
        self.summary_stack.addWidget(report_content)

        report_layout.addWidget(self.summary_stack)

        outer_layout.addWidget(report_widget)

        return outer_widget

    def create_report_item(self, label, value):
        widget = QWidget()
        layout = QVBoxLayout(widget)
        layout.setContentsMargins(0, 0, 0, 0)

        value_label = QLabel(value)
        value_label.setObjectName("reportValue")
        value_label.setAlignment(Qt.AlignLeft | Qt.AlignVCenter)

        description_label = QLabel(label)
        description_label.setObjectName("reportDescription")
        description_label.setAlignment(Qt.AlignLeft | Qt.AlignVCenter)

        layout.addWidget(value_label)
        layout.addWidget(description_label)

        return widget

    def setup_status_bar(self):
        self.status_bar = QStatusBar()
        self.setStatusBar(self.status_bar)
        self.update_status_bar()

    def apply_theme(self):
        self.setStyleSheet(self.theme_manager.get_stylesheet())
        self.update_icon_colors()

    def update_icon_colors(self):
        icon_color = QColor(
            self.theme_manager.custom_colors["icon_dark"]
            if self.theme_manager.dark_theme
            else self.theme_manager.custom_colors["icon_light"]
        )
        for action in self.toolbar.actions():
            if not action.isSeparator():
                icon = action.icon()
                if not icon.isNull():
                    pixmap = icon.pixmap(QSize(24, 24))
                    if not pixmap.isNull():
                        painter = QPainter(pixmap)
                        if painter.isActive():
                            painter.setCompositionMode(QPainter.CompositionMode_SourceIn)
                            painter.fillRect(pixmap.rect(), icon_color)
                            painter.end()
                            action.setIcon(QIcon(pixmap))

    def toggle_theme(self):
        self.theme_manager.toggle_theme()
        self.apply_theme()

    def update_status_bar(self):
        total_size_mb = self.file_manager.get_total_size() / (1024 * 1024)
        status_text = f"Files: {len(self.file_manager.documents)} | Total Size: {total_size_mb:.2f} MB | Status: {'Processing' if self.thread_pool.activeThreadCount() > 0 else 'Idle'}"
        self.status_bar.showMessage(status_text)
        if "Idle" in status_text:
            self.status_bar.clearMessage()

    def remove_selection_from_corpus(self):
        selected_text = self.processed_text.textCursor().selectedText()
        if selected_text:
            chars_to_remove = self.processor.parameters.get("chars_to_remove", [])
            if selected_text not in chars_to_remove:
                chars_to_remove.append(selected_text)
                self.processor.set_parameters({"chars_to_remove": chars_to_remove})
                QMessageBox.information(
                    self,
                    "Sequence Added",
                    f"'{selected_text}' has been added to sequences to remove.",
                )
                # Removed immediate processing to decouple registration from execution
                # Optional: Highlight the added selection without altering scroll position
                extra_selection = QTextEdit.ExtraSelection()
                extra_selection.format.setBackground(QColor("#FFCCCC"))  # Light red background
                extra_selection.cursor = self.processed_text.textCursor()
                extra_selection.cursor.clearSelection()
                self.processed_text.setExtraSelections([extra_selection])
            else:
                QMessageBox.information(
                    self,
                    "Sequence Already Exists",
                    f"'{selected_text}' is already in the sequences to remove.",
                )
        else:
            QMessageBox.warning(self, "No Selection", "Please select text to remove.")

    def show_processed_text_context_menu(self, pos):
        menu = self.processed_text.createStandardContextMenu()
        remove_action = QAction("Remove Selection from Corpus", self)
        remove_action.triggered.connect(self.remove_selection_from_corpus)
        menu.addAction(remove_action)
        menu.exec(self.processed_text.mapToGlobal(pos))

    def open_file(self):
        self.status_bar.clearMessage()
        files, _ = QFileDialog.getOpenFileNames(
            self, "Select files", "", supported_file_dialog_filter()
        )
        if files:
            start_time = time.time()
            new_documents = self.file_manager.add_files(files)
            if new_documents:
                self.file_list.setUpdatesEnabled(False)
                self.file_list.addItems([doc.file_path for doc in new_documents])
                self.file_list.setUpdatesEnabled(True)
                self.update_status_bar()
                elapsed = time.time() - start_time
                logging.info(f"File list populated: {len(new_documents)} items in {elapsed:.2f}s")
            else:
                QMessageBox.information(
                    self, "No New Files", "All selected files are already loaded."
                )

    def open_directory(self):
        self.status_bar.clearMessage()
        if hasattr(self, "loading_thread") and self.loading_thread.isRunning():
            self.cancel_loading()
        directory = QFileDialog.getExistingDirectory(self, "Select Directory")
        if directory:
            self.loading_dialog = FileLoadingDialog(self)
            self.loading_dialog.show()
            self.signals.update_progress.connect(self.loading_dialog.update_progress)
            self.loading_thread = QThread()
            self.loading_worker = DirectoryLoadingWorker(
                self.file_manager, directory, self.signals
            )
            self.loading_worker.moveToThread(self.loading_thread)
            self.loading_thread.started.connect(self.loading_worker.run)
            self.loading_worker.finished.connect(self.on_directory_loading_finished)
            self.loading_dialog.cancelled.connect(self.cancel_loading)
            self.loading_dialog.rejected.connect(self.cancel_loading)

            self.loading_thread.start()

    def cancel_loading(self):
        if hasattr(self, "loading_worker"):
            self.loading_worker.cancel()
        if hasattr(self, "loading_thread"):
            self.loading_thread.quit()
            self.loading_thread.wait()
        if hasattr(self, "loading_dialog") and self.loading_dialog.isVisible():
            self.loading_dialog.close()

    def on_directory_loading_finished(self, new_documents):
        self.loading_thread.quit()
        self.loading_thread.wait()
        if hasattr(self, "loading_dialog") and self.loading_dialog.isVisible():
            self.loading_dialog.close()
        if new_documents:
            start_time = time.time()
            self.file_list.setUpdatesEnabled(False)
            self.file_list.addItems([doc.file_path for doc in new_documents])
            self.file_list.setUpdatesEnabled(True)
            self.update_status_bar()
            elapsed = time.time() - start_time
            logging.info(f"File list populated: {len(new_documents)} items in {elapsed:.2f}s")
        else:
            self.status_bar.showMessage("Operation cancelled or no new files added.", 5000)

    def save_file(self):
        if not self.file_manager.documents:
            QMessageBox.warning(self, "Save Failed", "No files to save.")
            return

        modified_documents = [doc for doc in self.file_manager.documents if doc.is_modified]
        if not modified_documents:
            QMessageBox.information(self, "Save", "No modified files to save.")
            return

        structured_documents = [
            doc
            for doc in modified_documents
            if not can_overwrite_with_extracted_text(doc.file_path)
        ]
        if structured_documents:
            QMessageBox.information(
                self,
                "Export Required",
                (
                    "Save only overwrites plain .txt sources that can safely receive "
                    "processed text.\n\n"
                    "Structured sources such as HTML, DOCX, and PDF must be exported with "
                    "File > Export Processed Corpus... so originals are not overwritten "
                    "and manifests are written."
                ),
            )
            return

        confirmation_message = (
            "This will back up and overwrite the original .txt files with the "
            "processed text. Do you want to proceed?"
        )

        reply = QMessageBox.question(
            self,
            "Confirm Save",
            confirmation_message,
            QMessageBox.Yes | QMessageBox.No,
            QMessageBox.No,
        )
        if reply == QMessageBox.Yes:
            failed_files = []
            saved_in_place = 0

            for doc in modified_documents:
                try:
                    backup_path = f"{doc.file_path}.bak"
                    if not os.path.exists(backup_path):
                        shutil.copy2(doc.file_path, backup_path)
                    with open(doc.file_path, "w", encoding="utf-8") as file:
                        file.write(doc.processed_text)
                    saved_in_place += 1

                    doc.is_modified = False
                    self.mark_file_as_modified(doc)
                except Exception as e:
                    logging.error(f"Error saving file {doc.file_path}: {str(e)}")
                    failed_files.append(doc.file_path)

            if failed_files:
                QMessageBox.warning(
                    self,
                    "Save Completed with Errors",
                    f"Some files could not be saved:\n{', '.join(failed_files)}",
                )
            else:
                summary_parts = []
                if saved_in_place:
                    summary_parts.append(f"{saved_in_place} file(s) saved in place")
                QMessageBox.information(
                    self,
                    "Save Successful",
                    ". ".join(summary_parts) + ".",
                )

    def export_processed_corpus(self):
        if not self.file_manager.documents:
            QMessageBox.warning(self, "Export Failed", "No files to export.")
            return

        output_directory = QFileDialog.getExistingDirectory(self, "Export Processed Corpus")
        if not output_directory:
            self.status_bar.showMessage("Export cancelled.", 5000)
            return

        reply = QMessageBox.question(
            self,
            "Confirm Export",
            (
                "All loaded documents will be exported as UTF-8 .txt files under "
                "a texts folder. CorpusAid will also write manifest.json, "
                "warnings.json, warnings.csv, config.json, and README.txt.\n\n"
                "Original source files will not be modified. Continue?"
            ),
            QMessageBox.Yes | QMessageBox.No,
            QMessageBox.Yes,
        )
        if reply != QMessageBox.Yes:
            self.status_bar.showMessage("Export cancelled.", 5000)
            return

        self._set_busy_cursor(True)
        try:
            records = [
                processed_corpus_record_from_document(doc) for doc in self.file_manager.documents
            ]
            result = write_processed_corpus_export(
                records,
                self.processor.get_parameters(),
                output_directory,
                app_version=self.version,
            )
        except FileExistsError as exc:
            logging.error("Processed corpus export refused to overwrite files: %s", exc)
            QMessageBox.warning(
                self,
                "Export Failed",
                (
                    "CorpusAid will not overwrite an existing export. Choose an empty "
                    f"folder or remove the previous export files.\n\n{exc}"
                ),
            )
            return
        except Exception as exc:
            logging.exception("Processed corpus export failed")
            QMessageBox.warning(
                self,
                "Export Failed",
                f"Unable to export processed corpus:\n{exc}",
            )
            return
        finally:
            self._set_busy_cursor(False)

        for doc in self.file_manager.documents:
            doc.is_modified = False
            self.mark_file_as_modified(doc)

        self.status_bar.showMessage(
            f"Exported {result.files_exported} file(s) to {result.output_directory}",
            5000,
        )
        QMessageBox.information(
            self,
            "Export Successful",
            (
                f"Exported {result.files_exported} file(s).\n"
                f"Warnings recorded: {result.warning_count}.\n\n"
                f"Manifest: {result.manifest_path}"
            ),
        )

    def process_files(self):
        if not self.file_manager.documents:
            QMessageBox.warning(self, "No Files", "Please select files to process.")
            return

        # Show the processing dialog
        self.processing_dialog = FileLoadingDialog(self, title="Processing Files...")
        self.processing_dialog.show()
        self.signals.update_progress.connect(self.processing_dialog.update_progress)

        # Reset processing state
        self.processed_results.clear()
        self.errors.clear()
        self.warnings.clear()
        self.files_processed = 0
        self.total_size = self.file_manager.get_total_size()
        self.total_time = 0
        self.processed_size = 0
        self.active_workers = len(self.file_manager.documents)

        # Start processing each document
        for doc in self.file_manager.documents:
            worker = ProcessingWorker(self.processor, doc, self.signals)
            self.thread_pool.start(worker)

    def update_progress_info(self, message=None, error=None):
        with self.lock:
            progress = (
                (self.files_processed / len(self.file_manager.documents)) * 100
                if self.file_manager.documents
                else 0
            )
            avg_speed = self.processed_size / self.total_time if self.total_time > 0 else 0
            status_message = f"Progress: {progress:.2f}% | Avg. Speed: {avg_speed:.2f} B/s"
            if message:
                status_message += f" | {message}"
            if error:
                status_message += f" | Error: {error}"
            self.status_bar.showMessage(status_message)

    def handle_result(self, document, processed_text, processing_time):
        with self.lock:
            document.update_processed_text(processed_text)
            self.processed_results.append(
                (document.file_path, document.original_text, processed_text)
            )
            self.files_processed += 1
            self.processed_size += len(processed_text.encode("utf-8"))
            self.total_time += processing_time
            self.active_workers -= 1
        remaining_files = len(self.file_manager.documents) - self.files_processed
        self.update_progress_info(
            message=f"Processed {os.path.basename(document.file_path)} | Remaining files: {remaining_files}"
        )
        if self.active_workers == 0:
            self.on_all_workers_finished()

    def handle_error(self, file_path, error):
        with self.lock:
            self.errors.append((file_path, error))
            self.files_processed += 1
            self.active_workers -= 1
        remaining_files = len(self.file_manager.documents) - self.files_processed
        error_message = f"Error in {os.path.basename(file_path)}: {error} | Remaining files: {remaining_files}"
        self.update_progress_info(error=error_message)
        logging.error(f"Error processing {file_path}: {error}")
        if self.active_workers == 0:
            self.on_all_workers_finished()

    def handle_warning(self, file_path, warning):
        with self.lock:
            self.warnings.append((file_path, warning))
        warning_message = f"Warning in {os.path.basename(file_path)}: {warning}"
        self.update_progress_info(message=warning_message)
        logging.warning(f"Warning processing {file_path}: {warning}")

    def update_progress(self, current, total, time_remaining, error):
        with self.lock:
            self.files_processed = current
        self.update_progress_info()
        if error:
            self.status_bar.showMessage(f"Error: {error}", 5000)
            logging.error(f"Error during loading: {error}")

    def on_all_workers_finished(self):
        self.signals.processing_complete.emit(self.processed_results, self.warnings)
        if self.errors:
            error_msg = "\n".join([f"{file}: {error}" for file, error in self.errors])
            QMessageBox.warning(
                self,
                "Processing Completed with Errors",
                f"Errors occurred during processing:\n\n{error_msg}",
            )
        else:
            self.status_bar.clearMessage()
        self.update_status_bar()
        if hasattr(self, "processing_dialog") and self.processing_dialog.isVisible():
            self.processing_dialog.close()
        self.display_results()
        self._generate_report(processed=True)

    def stop_corpus_preview(self):
        """Legacy hook kept for compatibility; no-op in single-file view mode."""
        pass

    def display_results(self):
        self.file_list.clearSelection()
        self.refresh_display()

        if self.warnings:
            warning_msg = "\n".join(f"{file}: {warning}" for file, warning in self.warnings)
            QMessageBox.warning(
                self,
                "Processing Warnings",
                f"Warnings during processing:\n\n{warning_msg}",
            )

    def open_parameters_dialog(self):
        dialog = ParametersDialog(self.processor.get_parameters(), self)
        if dialog.exec():
            new_parameters = dialog.get_parameters()
            self.processor.set_parameters(new_parameters)
            self.processor.update_pipeline()
            self.status_bar.showMessage(
                "Processing settings updated. Click 'Preview Processed' to preview changes or 'Process Loaded Corpus' to process all loaded files.",
                10000,
            )

    def show_about_dialog(self):
        QMessageBox.about(
            self,
            "About",
            f"CorpusAid\nVersion {self.version}\n\nDeveloped by Jhonatan Lopes",
        )

    def show_documentation(self):
        documentation_file = self.resource_path("docs/documentation.html")
        if not os.path.exists(documentation_file):
            QMessageBox.warning(
                self,
                "Documentation Not Found",
                f"The documentation.html file could not be found: {documentation_file}",
            )
            return
        with open(documentation_file, "r", encoding="utf-8") as f:
            html_content = f.read()
        base_url = QUrl.fromLocalFile(os.path.abspath(documentation_file))

        bg_color = self.theme_manager.custom_colors["background"]
        text_color = self.theme_manager.custom_colors["text"]
        primary_color = self.theme_manager.custom_colors["primary"]
        secondary_color = self.theme_manager.custom_colors["secondary"]
        accent_color = self.theme_manager.custom_colors["accent"]

        css_styles = f"""
        <style>
            body {{
                background-color: {bg_color};
                color: {text_color};
                font-family: Roboto, sans-serif;
                margin: 20px;
            }}
            h1, h2, h3, h4, h5, h6 {{
                color: {primary_color};
            }}
            a {{
                color: {secondary_color};
            }}
            code, pre {{
                background-color: #444444;
                color: #f0f0f0;
                padding: 5px;
                border-radius: 5px;
                font-family: 'Courier New', Courier, monospace;
            }}
            pre {{
                padding: 10px;
            }}
        </style>
        """

        html_content = html_content.replace("</head>", f"{css_styles}</head>")

        if self.documentation_window is None:
            self.documentation_window = QWebEngineView()
            self.documentation_window.setWindowTitle("Documentation")
            self.documentation_window.resize(800, 600)
            self.documentation_window.setMinimumSize(800, 600)
            self.documentation_window.setWindowIcon(self.windowIcon())

        self.documentation_window.setStyleSheet(self.theme_manager.get_stylesheet())
        self.documentation_window.setHtml(html_content, base_url)
        self.documentation_window.show()

    def goto_occurrence(self, text_edit, index):
        if 0 <= index < len(self.search_results):
            cursor = QTextCursor(text_edit.document())
            cursor.setPosition(self.search_results[index].selectionStart())
            text_edit.setTextCursor(cursor)
            text_edit.centerCursor()

    def highlight_search_term(self, text_edit, search_term, current_index=-1):
        extra_selections = []
        self.search_results = []

        if not search_term:
            text_edit.setExtraSelections(extra_selections)
            self.update_occurrence_label()
            return

        highlight_format = QTextCharFormat()
        highlight_format.setBackground(QColor("#FFFF00"))
        highlight_format.setForeground(QColor("#000000"))
        highlight_format.setFontWeight(QFont.Bold)

        current_highlight_format = QTextCharFormat()
        current_highlight_format.setBackground(QColor("#FFA500"))
        current_highlight_format.setForeground(QColor("#000000"))
        current_highlight_format.setFontWeight(QFont.Bold)

        document = text_edit.document()
        cursor = QTextCursor(document)

        regex_pattern = QRegularExpression.escape(search_term)
        regex = QRegularExpression(regex_pattern)
        regex.setPatternOptions(QRegularExpression.CaseInsensitiveOption)
        matches = regex.globalMatch(document.toPlainText())

        while matches.hasNext():
            match = matches.next()
            match_cursor = QTextCursor(document)
            match_cursor.setPosition(match.capturedStart())
            match_cursor.setPosition(match.capturedEnd(), QTextCursor.KeepAnchor)

            selection = QTextEdit.ExtraSelection()
            selection.cursor = match_cursor

            if self.search_results and len(self.search_results) - 1 == current_index:
                selection.format = current_highlight_format
            else:
                selection.format = highlight_format

            extra_selections.append(selection)
            self.search_results.append(match_cursor)

        text_edit.setExtraSelections(extra_selections)
        self.update_occurrence_label()

    def search_text(self):
        search_term = self.search_input.text().strip()
        active_text_edit = self.text_tabs.currentWidget()
        if not isinstance(active_text_edit, QPlainTextEdit):
            return

        # If the search term is empty, clear highlights and exit
        if not search_term:
            self.search_results = []
            self.current_occurrence_index = -1
            self.update_occurrence_label()
            active_text_edit.setExtraSelections([])
            return

        if (
            search_term != self.previous_search_term
            or active_text_edit != self.previous_text_edit
        ):
            self.previous_search_term = search_term
            self.previous_text_edit = active_text_edit
            self.current_occurrence_index = 0

            self.highlight_search_term(
                active_text_edit, search_term, self.current_occurrence_index
            )
            if self.search_results:
                self.goto_occurrence(active_text_edit, self.current_occurrence_index)
                self.update_occurrence_label()
            else:
                self.current_occurrence_index = -1
                self.update_occurrence_label()
                QMessageBox.information(
                    self,
                    "Not Found",
                    f"The occurrence '{search_term}' was not found in the current text.",
                )
        else:
            if self.search_results:
                self.go_to_next_occurrence()
            else:
                QMessageBox.information(
                    self, "Not Found", f"The occurrence '{search_term}' was not found."
                )

    def go_to_next_occurrence(self):
        search_term = self.search_input.text().strip()
        active_text_edit = self.text_tabs.currentWidget()
        if not isinstance(active_text_edit, QPlainTextEdit):
            return

        # If the search term is empty, do nothing
        if not search_term:
            return

        if (
            not self.search_results
            or search_term != self.previous_search_term
            or active_text_edit != self.previous_text_edit
        ):
            self.search_text()
            return
        if not self.search_results:
            QMessageBox.information(
                self, "Not Found", f"The occurrence '{search_term}' was not found."
            )
            return

        self.current_occurrence_index = (self.current_occurrence_index + 1) % len(
            self.search_results
        )
        self.highlight_search_term(active_text_edit, search_term, self.current_occurrence_index)
        self.goto_occurrence(active_text_edit, self.current_occurrence_index)
        self.update_occurrence_label()

    def go_to_previous_occurrence(self):
        search_term = self.search_input.text().strip()
        active_text_edit = self.text_tabs.currentWidget()
        if not isinstance(active_text_edit, QPlainTextEdit):
            return

        # If the search term is empty, do nothing
        if not search_term:
            return

        if (
            not self.search_results
            or search_term != self.previous_search_term
            or active_text_edit != self.previous_text_edit
        ):
            self.search_text()
            return
        if not self.search_results:
            QMessageBox.information(
                self, "Not Found", f"The occurrence '{search_term}' was not found."
            )
            return

        self.current_occurrence_index = (self.current_occurrence_index - 1) % len(
            self.search_results
        )
        self.highlight_search_term(active_text_edit, search_term, self.current_occurrence_index)
        self.goto_occurrence(active_text_edit, self.current_occurrence_index)
        self.update_occurrence_label()

    def on_tab_changed(self, index):
        self.search_text()

    def update_occurrence_label(self):
        total = len(self.search_results)
        current = self.current_occurrence_index + 1 if self.current_occurrence_index >= 0 else 0
        self.occurrence_label.setText(f"{current}/{total}")

    def undo(self):
        selected_items = self.file_list.selectedItems()
        if selected_items:
            selected_file = selected_items[0].text().strip("* ")
            doc = self.file_manager.get_document_by_path(selected_file)
            if doc:
                doc.undo()
                self.refresh_display()
                self.mark_file_as_modified(doc)

    def redo(self):
        selected_items = self.file_list.selectedItems()
        if selected_items:
            selected_file = selected_items[0].text().strip("* ")
            doc = self.file_manager.get_document_by_path(selected_file)
            if doc:
                doc.redo()
                self.refresh_display()
                self.mark_file_as_modified(doc)

    def mark_file_as_modified(self, doc):
        for index in range(self.file_list.count()):
            item = self.file_list.item(index)
            if item.text().strip("* ") == doc.file_path:
                if doc.is_modified:
                    if not item.text().startswith("* "):
                        item.setText(f"* {doc.file_path}")
                else:
                    if item.text().startswith("* "):
                        item.setText(doc.file_path)
                break

    def _toggle_preview_controls(self, mode_text):
        enabled = mode_text == "First N files"
        self.preview_count_spin.setEnabled(enabled)

    def preview_original_action(self):
        self._trigger_preview(is_processed=False)

    def preview_processed_action(self):
        self._trigger_preview(is_processed=True)

    def _trigger_preview(self, is_processed):
        mode = self.preview_mode_combo.currentText()
        max_chars = self.preview_chars_spin.value()
        include_paths = self.preview_paths_check.isChecked()

        docs_to_preview = []
        if mode == "First N files":
            n_files = self.preview_count_spin.value()
            docs_to_preview = self.file_manager.documents[:n_files]
        elif mode == "Selected files":
            paths = [item.text().strip("* ") for item in self.file_list.selectedItems()]
            docs_to_preview = [self.file_manager.get_document_by_path(p) for p in paths if p]
            docs_to_preview = [d for d in docs_to_preview if d]
        elif mode == "Selected file":
            selected_items = self.file_list.selectedItems()
            if len(selected_items) == 1:
                path = selected_items[0].text().strip("* ")
                doc = self.file_manager.get_document_by_path(path)
                if doc:
                    docs_to_preview = [doc]
            elif len(selected_items) > 1:
                QMessageBox.warning(
                    self,
                    "Preview Error",
                    "Multiple files selected. Please switch preview mode to 'Selected files'.",
                )
                return
            else:
                QMessageBox.warning(self, "Preview Error", "Please select a file to preview.")
                return

        if not docs_to_preview:
            return

        self.original_text.clear()
        self.processed_text.clear()
        self._load_corpus_preview_async(docs_to_preview, max_chars, include_paths, is_processed)

    def _load_corpus_preview_async(self, documents, max_chars, include_paths, is_processed):
        self._cancel_document_loader()
        token = self._next_document_token()
        self.document_loader_worker = CorpusPreviewWorker(
            documents, token, max_chars, include_paths, is_processed
        )
        self.document_loader_thread = QThread()
        self.document_loader_worker.moveToThread(self.document_loader_thread)
        self.document_loader_thread.started.connect(self.document_loader_worker.run)
        self.document_loader_worker.finished.connect(self._handle_corpus_preview_loaded)
        self.document_loader_worker.error.connect(self._handle_document_error)
        self.document_loader_worker.finished.connect(lambda *_: self._cleanup_document_loader())
        self.document_loader_worker.error.connect(lambda *_: self._cleanup_document_loader())
        self.document_loader_thread.start()

        mode_str = "processed" if is_processed else "original"
        message = f"Generating {mode_str} preview for {len(documents)} files..."
        self._show_loading_indicator("document", token, message, determinate=False)
        self.status_bar.showMessage(message)

    def _handle_corpus_preview_loaded(self, token, original_preview, processed_preview):
        if token != self.document_request_token:
            return
        self._hide_loading_indicator("document", token)

        if original_preview:
            self.original_text.setPlainText(original_preview)
            self.text_tabs.setCurrentWidget(self.original_text)
        elif processed_preview:
            self.processed_text.setPlainText(processed_preview)
            self.text_tabs.setCurrentWidget(self.processed_text)

        self.status_bar.showMessage(f"Displaying corpus preview", 5000)

    def refresh_display(self):
        self.stop_corpus_preview()
        self._cancel_document_loader()

        selected_items = self.file_list.selectedItems()
        mode = (
            self.preview_mode_combo.currentText()
            if hasattr(self, "preview_mode_combo")
            else "Selected file"
        )

        if mode == "Selected files" and len(selected_items) > 1:
            self.current_file = None
            message = (
                f"{len(selected_items)} files selected.\n\n"
                "Click 'Preview Original' or 'Preview Processed' to generate a multi-file preview."
            )
            self.original_text.setPlainText(message)
            self.processed_text.setPlainText(message)
            self.status_bar.showMessage(f"{len(selected_items)} files selected.", 5000)
            return

        if len(selected_items) == 1:
            selected_file_path = selected_items[0].text().strip("* ")
            doc = self.file_manager.get_document_by_path(selected_file_path)
            if doc:
                self.current_file = doc.file_path
                if doc._original_text is not None:
                    original_text = doc._original_text
                    processed_text = (
                        doc._processed_text if doc._processed_text is not None else original_text
                    )
                    self.original_text.setPlainText(original_text)
                    self.processed_text.setPlainText(processed_text)
                    self.status_bar.showMessage(
                        f"Displaying: {os.path.basename(doc.file_path)}",
                        5000,
                    )
                else:
                    self.original_text.clear()
                    self.processed_text.clear()
                    self._load_document_async(doc)

        elif len(selected_items) > 1:
            self.current_file = None
            message = (
                f"{len(selected_items)} files selected.\n\n"
                "Please select a single file or switch preview mode to 'Selected files'."
            )
            self.original_text.setPlainText(message)
            self.processed_text.setPlainText(message)
            self.status_bar.showMessage(
                f"{len(selected_items)} files selected.",
                5000,
            )
        else:
            self.current_file = None
            if self.file_manager.documents:
                message = "Please select a file from the list to view its content."
                self.original_text.setPlainText(message)
                self.processed_text.setPlainText(message)
            else:
                self.original_text.clear()
                self.processed_text.clear()
            self.status_bar.clearMessage()

    def _next_document_token(self):
        self.document_request_token += 1
        return self.document_request_token

    def _load_document_async(self, doc):
        self._cancel_document_loader()
        token = self._next_document_token()
        self.document_loader_worker = DocumentLoaderWorker(
            doc.file_path, doc._processed_text, token
        )
        self.document_loader_thread = QThread()
        self.document_loader_worker.moveToThread(self.document_loader_thread)
        self.document_loader_thread.started.connect(self.document_loader_worker.run)
        self.document_loader_worker.finished.connect(self._handle_document_loaded)
        self.document_loader_worker.error.connect(self._handle_document_error)
        self.document_loader_worker.finished.connect(lambda *_: self._cleanup_document_loader())
        self.document_loader_worker.error.connect(lambda *_: self._cleanup_document_loader())
        self.document_loader_thread.start()
        message = f"Loading {os.path.basename(doc.file_path)}..."
        self._show_loading_indicator("document", token, message, determinate=False)
        self.status_bar.showMessage(message)

    def _handle_document_loaded(self, token, path, original_text, processed_text):
        if token != self.document_request_token:
            return
        self._hide_loading_indicator("document", token)
        doc = self.file_manager.get_document_by_path(path)
        if doc:
            doc._original_text = original_text
            if doc._processed_text is None:
                doc._processed_text = processed_text
            if not doc.history:
                doc.history = [doc._processed_text]
                doc.history_index = 0
            display_processed = (
                doc._processed_text if doc._processed_text is not None else processed_text
            )
        else:
            display_processed = processed_text
        self.original_text.setPlainText(original_text)
        self.processed_text.setPlainText(display_processed)
        self.status_bar.showMessage(f"Displaying: {os.path.basename(path)}", 5000)

    def _handle_document_error(self, token, path, message):
        if token != self.document_request_token:
            return
        self._hide_loading_indicator("document", token)
        logging.error("Failed to load %s: %s", path, message)
        self.original_text.clear()
        self.processed_text.clear()
        self.status_bar.showMessage(f"Failed to load {os.path.basename(path)}", 5000)
        QMessageBox.warning(
            self,
            "Load Error",
            f"Unable to load {os.path.basename(path)}:\\n{message}",
        )

    def _cleanup_document_loader(self):
        if self.document_loader_worker:
            self.document_loader_worker.deleteLater()
            self.document_loader_worker = None
        if self.document_loader_thread:
            if self.document_loader_thread.isRunning():
                self.document_loader_thread.quit()
                self.document_loader_thread.wait()
            self.document_loader_thread.deleteLater()
            self.document_loader_thread = None

    def _cancel_document_loader(self):
        active = False
        if self.document_loader_worker:
            self.document_loader_worker.cancel()
            active = True
        if self.document_loader_thread and self.document_loader_thread.isRunning():
            self.document_loader_thread.quit()
            self.document_loader_thread.wait()
            active = True
        self._cleanup_document_loader()
        if active and self._active_loading_token and self._active_loading_token[0] == "document":
            self._force_hide_loading_indicator()
            self.document_request_token += 1

    def confirm_start_new_cleaning(self):
        reply = QMessageBox.question(
            self,
            "Confirm New Project",
            "Are you sure you want to start a new project? All current data will be lost.",
            QMessageBox.Yes | QMessageBox.No,
            QMessageBox.No,
        )
        if reply == QMessageBox.Yes:
            self.start_new_cleaning()

    def start_new_cleaning(self):
        self.file_manager.clear_files()
        self.file_list.clear()
        self.stop_corpus_preview()
        self._cancel_document_loader()
        self.original_text.clear()
        self.processed_text.clear()
        self.current_file = None
        self.corpus_name = "Untitled Corpus"
        self.processor.reset_parameters()  # Ensure this method exists in DocumentProcessor
        self.clear_report()
        self.update_status_bar()

        if self.report_worker:
            self.report_thread.quit()
            self.report_thread.wait()
            self.report_worker = None

        QMessageBox.information(self, "New Project", "A new project has been started.")

    def clear_report(self):
        for key in self.report_items:
            self.report_items[key].findChild(QLabel, "reportValue").setText("0")
        self.summary_stack.setCurrentIndex(0)  # Show empty widget

    def update_report_display(self, report_data):
        self.summary_stack.setCurrentIndex(2)  # Show report content
        for key, value in report_data.items():
            if key in self.report_items:
                if isinstance(value, float):
                    formatted_value = f"{value:.2f}"
                else:
                    formatted_value = str(value)
                if key.endswith("_size"):
                    formatted_value += " MB"
                self.report_items[key].findChild(QLabel, "reportValue").setText(formatted_value)

    def display_original_texts(self):
        if self.file_manager.documents:
            self.file_list.clearSelection()
            self.refresh_display()
        else:
            self.original_text.clear()
            self.processed_text.clear()
            self.current_file = None
            self.status_bar.clearMessage()

    def _generate_report(self, processed=False):
        if self.report_worker is not None:
            return  # Report is already being generated
        parameters = self.processor.get_parameters()
        self.report_worker = ReportWorker(
            [doc.file_path for doc in self.file_manager.documents],
            parameters,
            processed,
            self.processed_results,
        )
        self.report_thread = QThread()
        self.report_worker.moveToThread(self.report_thread)
        self.report_worker.progress.connect(self.update_report_progress)
        self.report_worker.finished.connect(self.display_final_report)
        self.report_thread.started.connect(self.report_worker.run)
        self.report_thread.start()
        self.summary_stack.setCurrentIndex(1)  # Show loading widget

    def display_final_report(self, report_data):
        self.summary_stack.setCurrentIndex(2)  # Show report content
        for key, value in report_data.items():
            if key in self.report_items:
                if isinstance(value, float):
                    formatted_value = f"{value:.2f}"
                else:
                    formatted_value = str(value)
                if key.endswith("_size"):
                    formatted_value += " MB"
                self.report_items[key].findChild(QLabel, "reportValue").setText(formatted_value)
        self._cleanup_report_worker()

    def _cleanup_report_worker(self):
        if self.report_worker:
            self.report_worker.deleteLater()
            self.report_worker = None
        if self.report_thread:
            self.report_thread.quit()
            self.report_thread.wait()
            self.report_thread.deleteLater()
            self.report_thread = None

    def update_report_progress(self, progress):
        self.report_progress_bar.setValue(progress)
        self.loading_label.setText(f"Generating report... {progress}%")

    def handle_report_error(self, error_msg):
        logging.error(f"Error in report generation: {error_msg}")
        self.status_bar.showMessage("Error generating report", 5000)
        # Optionally, display error in the UI
        QMessageBox.warning(
            self,
            "Report Generation Error",
            f"An error occurred while generating the report:\n{error_msg}",
        )

    @Slot(list, list)
    def display_report(self, processed_results, warnings):
        # Generate report_data dictionary based on processed_results
        report_data = {
            "total_files": len(processed_results),
            "total_size": self.calculate_total_size(processed_results),
            "avg_size": self.calculate_average_size(processed_results),
            "total_words": self.calculate_total_words(processed_results),
            "avg_words": self.calculate_average_words(processed_results),
        }

        for key, value in report_data.items():
            if key in self.report_items:
                if isinstance(value, float):
                    formatted_value = f"{value:.2f}"
                else:
                    formatted_value = str(value)
                if key.endswith("_size"):
                    formatted_value += " MB"
                self.report_items[key].findChild(QLabel, "reportValue").setText(formatted_value)

    def calculate_total_size(self, processed_results):
        total_size = 0
        for _, _, processed in processed_results:
            total_size += len(processed.encode("utf-8"))
        return total_size / (1024 * 1024)  # Convert bytes to MB

    def calculate_average_size(self, processed_results):
        total_size = self.calculate_total_size(processed_results)
        count = len(processed_results)
        return total_size / count if count else 0

    def calculate_total_words(self, processed_results):
        total_words = 0
        for _, _, processed in processed_results:
            total_words += len(processed.split())
        return total_words

    def calculate_average_words(self, processed_results):
        total_words = self.calculate_total_words(processed_results)
        count = len(processed_results)
        return total_words / count if count else 0

    def check_for_updates(self, manual_trigger=False):
        try:
            url = "https://api.github.com/repos/jhlopesalves/CorpusAid/releases/latest"
            with urllib.request.urlopen(url) as response:
                data = response.read()
                latest_release = json.loads(data.decode("utf-8"))
                latest_version = latest_release["tag_name"]

                def parse_version(version_str):
                    return tuple(map(int, (version_str.strip("v").split("."))))

                current_version = parse_version(self.version)
                latest_version_parsed = parse_version(latest_version)

                if latest_version_parsed > current_version:
                    reply = QMessageBox.question(
                        self,
                        "Update Available",
                        f"A new version {latest_version} is available. Do you want to download it?",
                        QMessageBox.Yes | QMessageBox.No,
                        QMessageBox.Yes,
                    )
                    if reply == QMessageBox.Yes:
                        QDesktopServices.openUrl(QUrl(latest_release["html_url"]))
                elif manual_trigger:
                    QMessageBox.information(
                        self, "Up-to-Date", "You are using the latest version."
                    )
        except urllib.error.HTTPError as e:
            if e.code == 404 and manual_trigger:
                QMessageBox.information(
                    self,
                    "No Releases",
                    "No updates are available yet. You are using the latest version.",
                )
            elif e.code != 404:
                QMessageBox.warning(
                    self,
                    "Error",
                    f"An error occurred while checking for updates:\n{str(e)}",
                )
        except Exception as e:
            QMessageBox.warning(
                self,
                "Error",
                f"An error occurred while checking for updates:\n{str(e)}",
            )

    def closeEvent(self, event):
        self.stop_corpus_preview()
        self._cancel_document_loader()
        self.thread_pool.clear()
        self.thread_pool.waitForDone()
        if hasattr(self, "loading_thread") and self.loading_thread.isRunning():
            self.cancel_loading()
        if self.report_thread and self.report_thread.isRunning():
            self.report_thread.quit()
            self.report_thread.wait()
        event.accept()


def main():
    base_dir = os.path.dirname(os.path.abspath(__file__))
    log_dir = os.path.join(base_dir, "logs")
    if not os.path.exists(log_dir):
        os.makedirs(log_dir)
    log_file = os.path.join(log_dir, "CorpusAid.log")

    logging.basicConfig(
        filename=log_file,
        filemode="w",
        level=logging.DEBUG,
        format="%(asctime)s - %(levelname)s - %(message)s",
    )

    logging.info("Application started.")

    try:
        app = QApplication(sys.argv)

        # Set application icon
        icon_path = resource_path(os.path.join("assets", "my_icon.ico"))
        if os.path.exists(icon_path):
            app.setWindowIcon(QIcon(icon_path))
            logging.info(f"Application icon set successfully from {icon_path}")
        else:
            logging.error(f"Application icon not found at {icon_path}")

        window = PreprocessorGUI()
        window.show()
        exit_code = app.exec()
        logging.info("Application closed.")
        sys.exit(exit_code)
    except Exception as e:
        logging.exception("An error occurred: %s", e)


if __name__ == "__main__":
    main()
