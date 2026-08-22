namespace Ironpress;

/// <summary>A categorized failure returned by the Ironpress renderer.</summary>
public sealed class IronpressException : Exception
{
    internal IronpressException(IronpressErrorKind kind, int nativeStatus, string message)
        : base(message)
    {
        Kind = kind;
        NativeStatus = nativeStatus;
    }

    /// <summary>Gets the stable failure category.</summary>
    public IronpressErrorKind Kind { get; }

    /// <summary>Gets the numeric status returned by the native ABI.</summary>
    public int NativeStatus { get; }
}
