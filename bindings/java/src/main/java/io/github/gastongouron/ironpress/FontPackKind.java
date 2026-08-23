package io.github.gastongouron.ironpress;

/** The role of an optional Ironpress fallback-font pack. */
public enum FontPackKind {
  /** Japanese CJK fallback glyphs. */
  CJK_JAPANESE(1),

  /** Korean CJK and Hangul fallback glyphs. */
  CJK_KOREAN(2),

  /** Simplified Chinese CJK fallback glyphs. */
  CJK_SIMPLIFIED_CHINESE(3),

  /** Traditional Chinese CJK fallback glyphs. */
  CJK_TRADITIONAL_CHINESE(4),

  /** Monochrome outline emoji fallback glyphs. */
  EMOJI(5);

  private final int nativeValue;

  FontPackKind(int nativeValue) {
    this.nativeValue = nativeValue;
  }

  int nativeValue() {
    return nativeValue;
  }
}
