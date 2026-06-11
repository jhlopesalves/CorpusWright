"""Command-line interface for CorpusAid."""

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

from corpusaid.exporting import (
    ProcessedDocumentRecord,
    export_processed_corpus,
)
from corpusaid.ingestion import (
    UnsupportedDocumentTypeError,
    extract_document,
    supported_document_formats,
    supported_file_extensions,
)


def _json_safe(val: Any) -> Any:
    if isinstance(val, Path):
        return str(val)
    if isinstance(val, dict):
        return {str(k): _json_safe(v) for k, v in val.items()}
    if isinstance(val, (list, tuple, set)):
        return [_json_safe(v) for v in val]
    if isinstance(val, (str, int, float, bool)) or val is None:
        return val
    if hasattr(val, "value"):
        return str(val.value)
    return str(val)


def _handle_formats(args: argparse.Namespace) -> int:
    """Handle the 'formats' command."""
    formats_data = supported_document_formats()
    
    if args.json:
        print(json.dumps(formats_data, indent=2))
        return 0

    print(f"{'Extension':<12} {'Status':<15} {'Notes'}")
    print("-" * 50)
    
    rows = []
    for fmt in formats_data:
        status = fmt.get("support_level", "unknown")
        doc_type = fmt.get("document_type", "unknown")
        
        notes = ""
        if doc_type == "txt":
            notes = "Plain text"
        elif doc_type == "html":
            notes = "HTML plain-text extraction"
        elif doc_type == "docx":
            notes = "Main-body extraction"
        elif doc_type == "pdf":
            notes = "Embedded text extraction; no OCR"
            
        for ext in fmt.get("extensions", []):
            rows.append((ext, str(status), notes))
            
    rows.sort(key=lambda x: x[0])
    
    for ext, status, notes in rows:
        print(f"{ext:<12} {status:<15} {notes}")
        
    print("\nOptional fallback:")
    print("Tika server: disabled unless CORPUSAID_TIKA_SERVER_URL is set")
    
    return 0


def _handle_inspect(args: argparse.Namespace) -> int:
    """Handle the 'inspect' command."""
    try:
        path = Path(args.path)
        if not path.exists():
            print(
                json.dumps(
                    {
                        "ok": False,
                        "source_path": str(path),
                        "error_type": "FileNotFoundError",
                        "error": f"File not found: {path}",
                    },
                    indent=2,
                )
            )
            return 1
            
        document = extract_document(path)
        
        result: Dict[str, Any] = {
            "ok": True,
            "source_path": str(document.source_path),
            "document_type": document.document_type,
            "extraction_method": document.extraction_method,
            "character_count": len(document.text),
            "warnings": [
                {
                    "code": w.code,
                    "message": w.message,
                    "details": w.details,
                }
                for w in document.warnings
            ],
            "metadata": _json_safe(dict(document.metadata)),
        }
        
        for field in ("support_status", "quality", "chosen_backend", "fallback_attempted"):
            if field in document.metadata:
                result[field] = _json_safe(document.metadata[field])
                
        if "support_status" not in result:
            result["support_status"] = "unknown"
            
        print(json.dumps(result, indent=2))
        return 0
        
    except UnsupportedDocumentTypeError as e:
        print(
            json.dumps(
                {
                    "ok": False,
                    "source_path": str(args.path),
                    "error_type": "UnsupportedDocumentTypeError",
                    "error": str(e),
                },
                indent=2,
            )
        )
        return 1
    except Exception as e:
        print(
            json.dumps(
                {
                    "ok": False,
                    "source_path": str(args.path),
                    "error_type": type(e).__name__,
                    "error": str(e),
                },
                indent=2,
            )
        )
        return 1


def _handle_extract(args: argparse.Namespace) -> int:
    """Handle the 'extract' command."""
    try:
        source_path = Path(args.path)
        output_path = Path(args.output)
        
        if not source_path.exists():
            raise FileNotFoundError(f"Source file not found: {source_path}")
        if source_path.is_dir():
            raise IsADirectoryError(f"Source path is a directory: {source_path}")
            
        if output_path.suffix.lower() != ".txt":
            raise ValueError(f"Output must be a .txt file, got: {output_path.name}")
            
        try:
            if source_path.resolve() == output_path.resolve():
                raise ValueError("Source and output paths resolve to the same file.")
        except OSError:
            pass # Ignore resolution errors if file doesn't exist yet
            
        overwrote_existing_output = False
        if output_path.exists():
            if not args.force:
                raise FileExistsError("Output file already exists. Use --force to overwrite.")
            overwrote_existing_output = True
            
        document = extract_document(source_path)
        
        # Create missing parent directories safely
        output_path.parent.mkdir(parents=True, exist_ok=True)
        
        encoded_text = document.text.encode("utf-8")
        output_path.write_bytes(encoded_text)
        
        output_sha256 = hashlib.sha256(encoded_text).hexdigest()
        
        result: Dict[str, Any] = {
            "ok": True,
            "command": "extract",
            "source_path": str(document.source_path),
            "output_path": str(output_path),
            "document_type": document.document_type,
            "extraction_method": document.extraction_method,
            "character_count": len(document.text),
            "byte_count": len(encoded_text),
            "output_sha256": output_sha256,
            "overwrote_existing_output": overwrote_existing_output,
            "warnings": [
                {
                    "code": w.code,
                    "message": w.message,
                    "details": w.details,
                }
                for w in document.warnings
            ],
            "metadata": _json_safe(dict(document.metadata)),
        }
        
        for field in ("support_status", "quality", "chosen_backend", "fallback_attempted"):
            if field in document.metadata:
                result[field] = _json_safe(document.metadata[field])
                
        if "support_status" not in result:
            result["support_status"] = "unknown"
            
        print(json.dumps(result, indent=2))
        return 0
        
    except Exception as e:
        error_type = type(e).__name__
        if isinstance(e, UnsupportedDocumentTypeError):
            error_type = "UnsupportedDocumentTypeError"
            
        print(
            json.dumps(
                {
                    "ok": False,
                    "command": "extract",
                    "source_path": str(args.path),
                    "output_path": str(args.output),
                    "error_type": error_type,
                    "error": str(e),
                },
                indent=2,
            )
        )
        return 1


def _handle_export(args: argparse.Namespace) -> int:
    """Handle the 'export' command."""
    try:
        input_dir = Path(args.input_dir)
        output_dir = Path(args.output)
        
        if not input_dir.exists():
            raise FileNotFoundError(f"Input directory not found: {input_dir}")
        if not input_dir.is_dir():
            raise NotADirectoryError(f"Input path is not a directory: {input_dir}")
            
        try:
            resolved_in = input_dir.resolve()
            resolved_out = output_dir.resolve()
            if resolved_out == resolved_in:
                raise ValueError("Output directory cannot be the same as input directory.")
            if resolved_out.is_relative_to(resolved_in):
                raise ValueError("Output directory cannot be inside the input directory.")
        except OSError:
            pass # Ignore resolution errors if dir doesn't exist yet
            
        if output_dir.exists() and any(output_dir.iterdir()):
            raise FileExistsError("Output directory already exists and is not empty. The export service does not support overwriting.")

        formats = supported_document_formats()
        allowed_extensions = set()
        
        for fmt in formats:
            status = fmt.get("support_level", "unknown")
            is_stable = status == "stable"
            is_experimental = status == "experimental"
            
            if is_stable or (is_experimental and args.include_experimental):
                for ext in fmt.get("extensions", []):
                    allowed_extensions.add(ext.lower())

        discovered_files = []
        for path in input_dir.rglob("*"):
            if path.is_file() and path.suffix.lower() in allowed_extensions:
                discovered_files.append(path)
        
        discovered_files.sort()
        
        if not discovered_files:
            raise ValueError("No supported files found in the input directory.")

        records = []
        skipped = []
        
        for path in discovered_files:
            try:
                document = extract_document(path)
                records.append(ProcessedDocumentRecord.from_extracted_document(document))
            except Exception as e:
                skipped.append({
                    "path": str(path),
                    "reason": str(e),
                })
        
        config_payload = {
            "cli_export": True,
            "include_experimental": args.include_experimental,
        }
        
        export_result = export_processed_corpus(
            records=records,
            processing_config=config_payload,
            output_directory=output_dir,
            include_readme=True,
        )
        
        result = {
            "ok": True,
            "command": "export",
            "input_dir": str(input_dir),
            "output_dir": str(output_dir),
            "files_discovered": len(discovered_files),
            "files_exported": export_result.files_exported,
            "files_skipped": len(skipped),
            "include_experimental": args.include_experimental,
            "manifest_path": str(export_result.manifest_path),
            "warnings_path": str(export_result.warnings_path),
            "config_path": str(export_result.config_path),
            "texts_dir": str(export_result.texts_directory),
            "warnings_count": export_result.warning_count,
            "skipped": skipped,
        }
        
        if export_result.warnings_csv_path.exists():
            result["warnings_csv_path"] = str(export_result.warnings_csv_path)
        if export_result.readme_path and export_result.readme_path.exists():
            result["readme_path"] = str(export_result.readme_path)
            
        print(json.dumps(result, indent=2))
        return 0
        
    except Exception as e:
        error_type = type(e).__name__
        print(
            json.dumps(
                {
                    "ok": False,
                    "command": "export",
                    "input_dir": str(args.input_dir),
                    "output_dir": str(args.output),
                    "error_type": error_type,
                    "error": str(e),
                },
                indent=2,
            )
        )
        return 1


def main(argv: Optional[List[str]] = None) -> int:
    """Main CLI entry point."""
    parser = argparse.ArgumentParser(
        description="CorpusAid command-line interface",
        prog="corpusaid",
    )
    subparsers = parser.add_subparsers(title="commands", dest="command")
    
    formats_parser = subparsers.add_parser(
        "formats",
        help="List currently registered document formats and support levels",
    )
    formats_parser.add_argument(
        "--json", 
        action="store_true", 
        help="Output format support data as JSON",
    )
    
    inspect_parser = subparsers.add_parser(
        "inspect",
        help="Inspect a document through the ingestion layer and print a JSON summary",
    )
    inspect_parser.add_argument("path", help="Path to the document to inspect")
    
    extract_parser = subparsers.add_parser(
        "extract",
        help="Extract a single document to a .txt file",
    )
    extract_parser.add_argument("path", help="Path to the document to extract")
    extract_parser.add_argument("--output", required=True, help="Output .txt file path")
    extract_parser.add_argument("--force", action="store_true", help="Overwrite existing output file")
    
    export_parser = subparsers.add_parser(
        "export",
        help="Export a directory of documents to a processed corpus",
    )
    export_parser.add_argument("input_dir", help="Directory containing documents to export")
    export_parser.add_argument("--output", required=True, help="Output directory for the exported corpus")
    export_parser.add_argument("--include-experimental", action="store_true", help="Include experimental formats like .docx and .pdf")
    
    args = parser.parse_args(argv)
    
    if args.command == "formats":
        return _handle_formats(args)
    elif args.command == "inspect":
        return _handle_inspect(args)
    elif args.command == "extract":
        return _handle_extract(args)
    elif args.command == "export":
        return _handle_export(args)
        
    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
