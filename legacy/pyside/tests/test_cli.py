"""Tests for the CorpusAid command-line interface."""

import json
from pathlib import Path
from corpusaid.cli import main


def test_formats_command_plain(capsys):
    result = main(["formats"])
    assert result == 0
    captured = capsys.readouterr()
    output = captured.out
    assert ".txt" in output
    assert ".html" in output
    assert ".htm" in output
    assert ".docx" in output
    assert ".pdf" in output
    
    lines = output.split('\n')
    first_words = [line.split()[0] for line in lines if line.strip()]
    assert ".doc" not in first_words
    assert ".epub" not in first_words
    assert "OCR" not in first_words
    assert "Pdfium" not in first_words
    
    assert "Optional fallback:" in output
    assert "CORPUSAID_TIKA_SERVER_URL" in output
    assert "stable" in output
    assert "experimental" in output


def test_formats_command_json(capsys):
    result = main(["formats", "--json"])
    assert result == 0
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert isinstance(data, list)
    extensions_found = set()
    for fmt in data:
        extensions = fmt.get("extensions", [])
        extensions_found.update(extensions)
    assert ".txt" in extensions_found
    assert ".html" in extensions_found
    assert ".htm" in extensions_found
    assert ".docx" in extensions_found
    assert ".pdf" in extensions_found
    assert ".doc" not in extensions_found
    assert ".epub" not in extensions_found


def test_main_no_args(capsys):
    result = main([])
    assert result == 1
    captured = capsys.readouterr()
    assert "usage:" in captured.out
    assert "commands:" in captured.out


def test_inspect_txt_success(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.txt"
    doc_path.write_text("Hello CorpusAid", encoding="utf-8")
    result = main(["inspect", str(doc_path)])
    assert result == 0
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is True
    assert data["document_type"] == "txt"
    assert data["support_status"] == "stable"
    assert data["character_count"] == 15
    assert "Hello CorpusAid" not in captured.out


def test_inspect_html_success(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.html"
    doc_path.write_text("<html><body>Hello</body></html>", encoding="utf-8")
    result = main(["inspect", str(doc_path)])
    assert result == 0
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is True
    assert data["support_status"] == "stable"
    assert data["character_count"] > 0


def test_inspect_unsupported(capsys, tmp_path):
    doc_path = tmp_path / "test.epub"
    doc_path.write_text("fake epub content", encoding="utf-8")
    result = main(["inspect", str(doc_path)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False
    assert "error_type" in data
    assert "UnsupportedDocumentTypeError" in data["error_type"]
    assert "error" in data


def test_inspect_missing_file(capsys, tmp_path):
    doc_path = tmp_path / "missing.txt"
    result = main(["inspect", str(doc_path)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False
    assert data["error_type"] == "FileNotFoundError"


def test_extract_txt_success(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.txt"
    out_path = tmp_path / "out.txt"
    doc_path.write_text("Hello Extract", encoding="utf-8")
    
    result = main(["extract", str(doc_path), "--output", str(out_path)])
    assert result == 0
    
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is True
    assert data["command"] == "extract"
    assert data["document_type"] == "txt"
    assert data["support_status"] == "stable"
    assert data["character_count"] == 13
    assert "output_sha256" in data
    assert "Hello Extract" not in captured.out
    
    assert out_path.exists()
    assert out_path.read_text(encoding="utf-8") == "Hello Extract"


def test_extract_html_success(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.html"
    out_path = tmp_path / "out.txt"
    doc_path.write_text("<html><body>Hello Extract</body></html>", encoding="utf-8")
    
    result = main(["extract", str(doc_path), "--output", str(out_path)])
    assert result == 0
    
    assert out_path.exists()
    assert "Hello Extract" in out_path.read_text(encoding="utf-8")
    
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is True
    assert data["support_status"] == "stable"
    assert "Hello Extract" not in captured.out


def test_extract_already_exists_no_force(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.txt"
    out_path = tmp_path / "out.txt"
    doc_path.write_text("Hello Extract", encoding="utf-8")
    out_path.write_text("Old Content", encoding="utf-8")
    
    result = main(["extract", str(doc_path), "--output", str(out_path)])
    assert result == 1
    
    assert out_path.read_text(encoding="utf-8") == "Old Content"
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False
    assert data["error_type"] == "FileExistsError"


def test_extract_already_exists_with_force(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.txt"
    out_path = tmp_path / "out.txt"
    doc_path.write_text("Hello Extract", encoding="utf-8")
    out_path.write_text("Old Content", encoding="utf-8")
    
    result = main(["extract", str(doc_path), "--output", str(out_path), "--force"])
    assert result == 0
    
    assert out_path.read_text(encoding="utf-8") == "Hello Extract"
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is True
    assert data["overwrote_existing_output"] is True


def test_extract_same_file(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.txt"
    doc_path.write_text("Hello Extract", encoding="utf-8")
    
    result = main(["extract", str(doc_path), "--output", str(doc_path)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False
    assert data["error_type"] == "ValueError"


def test_extract_not_txt_suffix(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.txt"
    out_path = tmp_path / "out.csv"
    doc_path.write_text("Hello Extract", encoding="utf-8")
    
    result = main(["extract", str(doc_path), "--output", str(out_path)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False
    assert data["error_type"] == "ValueError"
    assert not out_path.exists()


def test_extract_unsupported(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "test.epub"
    out_path = tmp_path / "out.txt"
    doc_path.write_text("fake epub", encoding="utf-8")
    
    result = main(["extract", str(doc_path), "--output", str(out_path)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_extract_missing_source(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    doc_path = tmp_path / "missing.txt"
    out_path = tmp_path / "out.txt"
    
    result = main(["extract", str(doc_path), "--output", str(out_path)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_extract_directory_source(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    dir_path = tmp_path / "dir"
    dir_path.mkdir()
    out_path = tmp_path / "out.txt"
    
    result = main(["extract", str(dir_path), "--output", str(out_path)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_export_success_stable(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    in_dir = tmp_path / "in"
    in_dir.mkdir()
    out_dir = tmp_path / "out"
    
    (in_dir / "doc1.txt").write_text("Doc 1", encoding="utf-8")
    (in_dir / "doc2.html").write_text("<html>Doc 2</html>", encoding="utf-8")
    (in_dir / "unsupported.epub").write_text("fake", encoding="utf-8")
    
    result = main(["export", str(in_dir), "--output", str(out_dir)])
    assert result == 0
    
    assert out_dir.exists()
    assert (out_dir / "texts").exists()
    assert (out_dir / "manifest.json").exists()
    assert (out_dir / "warnings.json").exists()
    assert (out_dir / "config.json").exists()
    
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is True
    assert data["command"] == "export"
    assert data["files_discovered"] == 2
    assert data["files_exported"] == 2
    assert data["files_skipped"] == 0
    assert data["include_experimental"] is False


def test_export_missing_input(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    in_dir = tmp_path / "missing"
    out_dir = tmp_path / "out"
    
    result = main(["export", str(in_dir), "--output", str(out_dir)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_export_input_is_file(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    in_dir = tmp_path / "file.txt"
    in_dir.write_text("content", encoding="utf-8")
    out_dir = tmp_path / "out"
    
    result = main(["export", str(in_dir), "--output", str(out_dir)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_export_output_equals_input(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    in_dir = tmp_path / "in"
    in_dir.mkdir()
    
    result = main(["export", str(in_dir), "--output", str(in_dir)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_export_output_inside_input(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    in_dir = tmp_path / "in"
    in_dir.mkdir()
    out_dir = in_dir / "out"
    
    result = main(["export", str(in_dir), "--output", str(out_dir)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_export_existing_output(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    in_dir = tmp_path / "in"
    in_dir.mkdir()
    out_dir = tmp_path / "out"
    out_dir.mkdir()
    (out_dir / "existing.txt").touch()
    
    result = main(["export", str(in_dir), "--output", str(out_dir)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False


def test_export_no_supported_files(capsys, tmp_path, monkeypatch):
    monkeypatch.delenv("CORPUSAID_TIKA_SERVER_URL", raising=False)
    in_dir = tmp_path / "in"
    in_dir.mkdir()
    out_dir = tmp_path / "out"
    
    (in_dir / "unsupported.epub").write_text("fake", encoding="utf-8")
    
    result = main(["export", str(in_dir), "--output", str(out_dir)])
    assert result == 1
    captured = capsys.readouterr()
    data = json.loads(captured.out)
    assert data["ok"] is False
