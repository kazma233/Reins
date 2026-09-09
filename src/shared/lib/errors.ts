export function extractErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }

  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }

  if (error && typeof error === "object") {
    const message = Reflect.get(error, "message");

    if (typeof message === "string" && message.trim().length > 0) {
      return message;
    }

    try {
      const serialized = JSON.stringify(error, null, 2);

      if (serialized && serialized !== "{}") {
        return serialized;
      }
    } catch {
      return fallback;
    }
  }

  const nextMessage = String(error ?? "").trim();
  return nextMessage.length > 0 ? nextMessage : fallback;
}
