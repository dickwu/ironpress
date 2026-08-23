package io.github.gastongouron.ironpress;

/** A stable machine-readable Ironpress failure category. */
public enum IronpressErrorKind {
  /** An unknown status from an incompatible native library. */
  UNKNOWN(-1),
  /** An argument violates the native contract. */
  INVALID_ARGUMENT(1),
  /** Text is not valid UTF-8. */
  INVALID_UTF8(2),
  /** An integer does not identify a documented value. */
  INVALID_ENUM(3),
  /** A required native owner is absent. */
  INVALID_HANDLE(4),
  /** An output owner was not empty. */
  OUTPUT_NOT_EMPTY(5),
  /** The HTML parser rejected the document. */
  PARSE(10),
  /** The CSS parser rejected the document. */
  CSS(11),
  /** The layout engine could not lay out the document. */
  LAYOUT(12),
  /** The PDF renderer could not produce the document. */
  RENDER(13),
  /** A font or fallback pack could not be parsed or embedded. */
  FONT(14),
  /** A filesystem operation failed. */
  IO(15),
  /** The resource-security policy rejected the document. */
  SECURITY(16),
  /** The native boundary caught an unexpected internal failure. */
  INTERNAL(255);

  private final int nativeStatus;

  IronpressErrorKind(int nativeStatus) {
    this.nativeStatus = nativeStatus;
  }

  static IronpressErrorKind fromNative(int status) {
    for (var kind : values()) {
      if (kind.nativeStatus == status) {
        return kind;
      }
    }
    return UNKNOWN;
  }
}
