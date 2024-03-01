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
