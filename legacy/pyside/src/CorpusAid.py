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
