namespace Ironpress.Interop;

internal enum NativeStatus
{
    Ok = 0,
    InvalidArgument = 1,
    InvalidUtf8 = 2,
    InvalidEnum = 3,
    InvalidHandle = 4,
    OutputNotEmpty = 5,
    Parse = 10,
    Css = 11,
    Layout = 12,
    Render = 13,
    Font = 14,
    Io = 15,
    Security = 16,
    Internal = 255,
}
