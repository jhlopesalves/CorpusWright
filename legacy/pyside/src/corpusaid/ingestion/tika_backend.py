"""Optional Apache Tika server fallback backend."""

from __future__ import annotations

import mimetypes
import os
from pathlib import Path
import time
from typing import Callable, Optional
import urllib.error
import urllib.request

from .models import ExtractionBackendResult, ExtractionWarning

UrlOpen = Callable[..., object]


class TikaServerBackend:
    """Experimental fallback that calls a user-provided Tika server URL."""

    backend_name = "tika:server"
    support_status = "experimental"

    def __init__(
        self,
        server_url: str,
        *,
        endpoint: str = "/tika",
        timeout_seconds: float = 10.0,
        urlopen: UrlOpen = urllib.request.urlopen,
    ):
        self.server_url = server_url.rstrip("/")
        self.endpoint = endpoint or "/tika"
        if not self.endpoint.startswith("/"):
            self.endpoint = f"/{self.endpoint}"
        self.timeout_seconds = timeout_seconds
        self.urlopen = urlopen

    @classmethod
    def from_environment(cls) -> Optional["TikaServerBackend"]:
        server_url = os.environ.get("CORPUSAID_TIKA_SERVER_URL", "").strip()
        if not server_url:
            return None

        endpoint = os.environ.get("CORPUSAID_TIKA_ENDPOINT", "/tika").strip() or "/tika"
        timeout_raw = os.environ.get("CORPUSAID_TIKA_TIMEOUT_SECONDS", "10").strip()
        try:
            timeout = float(timeout_raw)
        except ValueError:
            timeout = 10.0

        return cls(server_url, endpoint=endpoint, timeout_seconds=timeout)

    def extract(self, path: Path, *, fallback_reason: str) -> ExtractionBackendResult:
        start = time.perf_counter()
        metadata = {
            "backend": "tika",
            "fallback_reason": fallback_reason,
            "tika_endpoint": self.endpoint,
        }

        try:
            data = path.read_bytes()
            request = urllib.request.Request(
                self._url(),
                data=data,
                method="PUT",
                headers={
                    "Accept": "text/plain",
                    "Content-Type": _content_type(path),
                },
            )
            response = self.urlopen(request, timeout=self.timeout_seconds)
            with response:
                status = int(getattr(response, "status", 200))
                body = response.read()
            if status >= 400:
                raise urllib.error.HTTPError(
                    self._url(), status, "Tika server returned an error", {}, None
                )
            text = body.decode("utf-8", errors="replace")
            return ExtractionBackendResult(
                backend_name=self.backend_name,
                success=True,
                text=text,
                warnings=[],
                metadata=metadata,
                elapsed_seconds=time.perf_counter() - start,
                support_status=self.support_status,
            )
        except urllib.error.HTTPError as exc:
            return self._failed_result(
                exc,
                code="tika_fallback_failed",
                message="Configured Tika fallback failed",
                metadata=metadata,
                start=start,
            )
        except (urllib.error.URLError, OSError) as exc:
            return self._failed_result(
                exc,
                code="tika_fallback_unavailable",
                message="Configured Tika fallback is unavailable",
                metadata=metadata,
                start=start,
            )
        except Exception as exc:  # pragma: no cover - defensive path
            return self._failed_result(
                exc,
                code="tika_fallback_failed",
                message="Configured Tika fallback failed",
                metadata=metadata,
                start=start,
            )

    def _url(self) -> str:
        return f"{self.server_url}{self.endpoint}"

    def _failed_result(
        self,
        exc: Exception,
        *,
        code: str,
        message: str,
        metadata: dict,
        start: float,
    ) -> ExtractionBackendResult:
        return ExtractionBackendResult(
            backend_name=self.backend_name,
            success=False,
            text="",
            warnings=[
                ExtractionWarning(
                    code=code,
                    message=message,
                    details=str(exc),
                )
            ],
            metadata=metadata,
            elapsed_seconds=time.perf_counter() - start,
            support_status=self.support_status,
            error=str(exc),
        )


def _content_type(path: Path) -> str:
    content_type, _encoding = mimetypes.guess_type(str(path))
    return content_type or "application/octet-stream"
