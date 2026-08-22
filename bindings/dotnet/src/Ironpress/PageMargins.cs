namespace Ironpress;

/// <summary>Finite physical page margins measured in points.</summary>
public sealed class PageMargins
{
    private PageMargins(float top, float right, float bottom, float left)
    {
        Top = top;
        Right = right;
        Bottom = bottom;
        Left = left;
    }

    /// <summary>Gets the top margin in points.</summary>
    public float Top { get; }

    /// <summary>Gets the right margin in points.</summary>
    public float Right { get; }

    /// <summary>Gets the bottom margin in points.</summary>
    public float Bottom { get; }

    /// <summary>Gets the left margin in points.</summary>
    public float Left { get; }

    /// <summary>Create four equal physical margins.</summary>
    public static PageMargins Uniform(float points) =>
        FromPoints(points, points, points, points);

    /// <summary>Create physical margins in CSS clockwise order.</summary>
    public static PageMargins FromPoints(float top, float right, float bottom, float left)
    {
        EnsureFinite(top, nameof(top));
        EnsureFinite(right, nameof(right));
        EnsureFinite(bottom, nameof(bottom));
        EnsureFinite(left, nameof(left));
        return new PageMargins(top, right, bottom, left);
    }

    private static void EnsureFinite(float value, string parameterName)
    {
        if (!float.IsFinite(value))
        {
            throw new ArgumentOutOfRangeException(parameterName, "Page margins must be finite.");
        }
    }
}
