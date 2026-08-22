using System.Text;

namespace Ironpress.Interop;

internal static class Utf8Input
{
    private static readonly Encoding StrictEncoding = new UTF8Encoding(false, true);

    internal static byte[] Encode(string value, string parameterName)
    {
        ArgumentNullException.ThrowIfNull(value, parameterName);

        try
        {
            return StrictEncoding.GetBytes(value);
        }
        catch (EncoderFallbackException error)
        {
            throw new ArgumentException(
                "Text must contain valid Unicode scalar values.",
                parameterName,
                error);
        }
    }
}
