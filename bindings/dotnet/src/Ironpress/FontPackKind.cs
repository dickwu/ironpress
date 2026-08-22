namespace Ironpress;

/// <summary>The role of an optional Ironpress fallback-font pack.</summary>
public enum FontPackKind
{
    /// <summary>Japanese CJK fallback glyphs.</summary>
    CjkJapanese = 1,

    /// <summary>Korean CJK and Hangul fallback glyphs.</summary>
    CjkKorean = 2,

    /// <summary>Simplified Chinese CJK fallback glyphs.</summary>
    CjkSimplifiedChinese = 3,

    /// <summary>Traditional Chinese CJK fallback glyphs.</summary>
    CjkTraditionalChinese = 4,

    /// <summary>Monochrome outline emoji fallback glyphs.</summary>
    Emoji = 5,
}
