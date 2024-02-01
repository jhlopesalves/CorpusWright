import logging
import re
import unicodedata

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
