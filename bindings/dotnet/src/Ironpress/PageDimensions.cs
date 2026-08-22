namespace Ironpress;

/// <summary>A positive finite custom page size measured in points.</summary>
public sealed class PageDimensions
{
    private PageDimensions(float width, float height)
    {
        Width = width;
        Height = height;
    }

    /// <summary>Gets the page width in points.</summary>
    public float Width { get; }

    /// <summary>Gets the page height in points.</summary>
    public float Height { get; }

    /// <summary>Create a custom page size from physical point dimensions.</summary>
    /// <param name="width">Positive finite width in points.</param>
    /// <param name="height">Positive finite height in points.</param>
    /// <returns>A validated page-size value.</returns>
    /// <exception cref="ArgumentOutOfRangeException">
    /// A dimension is zero, negative, NaN, or infinite.
    /// </exception>
    public static PageDimensions FromPoints(float width, float height)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(width);
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(height);

        if (!float.IsFinite(width))
        {
            throw new ArgumentOutOfRangeException(nameof(width), "Page width must be finite.");
        }

        if (!float.IsFinite(height))
        {
            throw new ArgumentOutOfRangeException(nameof(height), "Page height must be finite.");
        }

        return new PageDimensions(width, height);
    }
}
