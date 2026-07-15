export type IpcErrorCode =
  | "invalid_input"
  | "unsupported_input"
  | "cancelled"
  | "stale_result"
  | "resource_unavailable"
  | "resource_limit"
  | "corrupt_persistent_state"
  | "native_failure"
  | "internal_invariant";

export interface NormalisedIpcError {
  code: IpcErrorCode;
  message: string;
  retryable: boolean;
}

export function normaliseIpcError(error: unknown): NormalisedIpcError {
  const message = error instanceof Error ? error.message : `${error}`;
  const lower = message.toLowerCase();
  if (lower.includes("cancel")) return classified("cancelled", message, true);
  if (lower.includes("reloaded") || lower.includes("stale")) {
    return classified("stale_result", message, true);
  }
  if (lower.includes("unsupported")) return classified("unsupported_input", message, false);
  if (lower.includes("memory limit") || lower.includes("resource limit")) {
    return classified("resource_limit", message, true);
  }
  if (lower.includes("corrupt") || lower.includes("version mismatch")) {
    return classified("corrupt_persistent_state", message, false);
  }
  if (lower.includes("pdfium") || lower.includes("ocr") || lower.includes("native")) {
    return classified("native_failure", message, true);
  }
  if (lower.includes("not available") || lower.includes("not found")) {
    return classified("resource_unavailable", message, true);
  }
  if (lower.includes("invalid") || lower.includes("choose ") || lower.includes("must ")) {
    return classified("invalid_input", message, false);
  }
  return classified("internal_invariant", message, true);
}

function classified(
  code: IpcErrorCode,
  message: string,
  retryable: boolean,
): NormalisedIpcError {
  return { code, message, retryable };
}
