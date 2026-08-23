package io.github.gastongouron.ironpress;

/** A categorized failure returned by the Ironpress renderer. */
public final class IronpressException extends RuntimeException {
  private static final long serialVersionUID = 1L;

  private final IronpressErrorKind kind;
  private final int nativeStatus;

  IronpressException(IronpressErrorKind kind, int nativeStatus, String message) {
    super(message);
    this.kind = kind;
    this.nativeStatus = nativeStatus;
  }

  /** Return the stable failure category. */
  public IronpressErrorKind getKind() {
    return kind;
  }

  /** Return the numeric status produced by the native ABI. */
  public int getNativeStatus() {
    return nativeStatus;
  }
}
