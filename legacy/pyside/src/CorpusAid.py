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
