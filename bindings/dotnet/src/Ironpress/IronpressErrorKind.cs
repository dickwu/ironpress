namespace Ironpress;

/// <summary>A stable machine-readable Ironpress failure category.</summary>
public enum IronpressErrorKind
{
    /// <summary>An unknown status from an incompatible native library.</summary>
    Unknown = -1,

    /// <summary>An argument violates the native contract.</summary>
    InvalidArgument = 1,

    /// <summary>Text is not valid UTF-8.</summary>
    InvalidUtf8 = 2,

    /// <summary>An integer does not identify a documented value.</summary>
    InvalidEnum = 3,

    /// <summary>A required native owner is absent.</summary>
    InvalidHandle = 4,

    /// <summary>An output owner was not empty.</summary>
    OutputNotEmpty = 5,

    /// <summary>The HTML parser rejected the document.</summary>
    Parse = 10,

    /// <summary>The CSS parser rejected the document.</summary>
    Css = 11,

    /// <summary>The layout engine could not lay out the document.</summary>
    Layout = 12,

    /// <summary>The PDF renderer could not produce the document.</summary>
    Render = 13,

    /// <summary>A font or fallback pack could not be parsed or embedded.</summary>
    Font = 14,

    /// <summary>A filesystem operation failed.</summary>
    Io = 15,

    /// <summary>The resource-security policy rejected the document.</summary>
    Security = 16,

    /// <summary>The native boundary caught an unexpected internal failure.</summary>
    Internal = 255,
}
