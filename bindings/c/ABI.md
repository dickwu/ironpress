# Ironpress C ABI

The C binding is the native contract shared by C, C++, and .NET. Java may use
the same contract through JNA or a thin JNI adapter. The ABI translates values,
ownership, and failures only; all document behavior remains in the `ironpress`
crate.

## Compatibility

`ironpress_abi_version()` returns the ABI generation. Generation 1 may gain new
symbols and status codes, but existing function signatures, numeric constants,
and ownership rules will not change. A breaking contract requires a new ABI
generation and a new set of exported symbols.

The package version is available through `ironpress_version()`. Package and ABI
versions are independent: compatible Ironpress releases retain ABI generation 1.

## Values

Text and binary inputs use `IronpressBytes`, a pointer and a byte length. A null
pointer is valid only when the length is zero. Text is decoded as UTF-8 at the
boundary. Invalid UTF-8, enum discriminants, booleans, and non-finite geometry
return a stable status before the renderer is called.

No Rust string, vector, reference, enum layout, or allocator crosses the ABI.
Page-size and font-pack constants are fixed-width integers.

## Ownership

Converters, PDF buffers, and errors are opaque handles. Every successful
allocation has exactly one owner and one matching free function. Free functions
take a pointer to the caller's handle, release the allocation, and replace the
handle with null. Repeating the free operation through that same handle is safe.

Copying an owning handle does not create a second owner. A copied handle becomes
invalid when the original owner frees the allocation. Passing arbitrary, stale,
or foreign pointers is undefined behavior, as with other opaque C APIs.

PDF bytes and error-message bytes are borrowed views into their owning handles.
They remain valid until that handle is freed and must never be freed directly.

## Failures

Fallible functions return an `IronpressStatus`. When `out_error` is non-null,
the caller must initialize its value to null. A failure then installs an owned
`IronpressError` containing the same status and a UTF-8 message. The message
explains the invalid input or maps the renderer's error category without leaking
Rust implementation layouts.

All exported operations contain Rust panics. An unexpected panic becomes
`IRONPRESS_STATUS_INTERNAL` and never unwinds into foreign code.

## Threads

A converter has one owner. It may move between threads while idle, but the same
handle must not be read, configured, converted, or freed concurrently. Returned
buffers and errors follow the same exclusive-lifetime rule.

## Scope of generation 1

The initial contract includes HTML and Markdown conversion, reusable converters,
page geometry, quality controls, sanitization, headers, footers, custom TrueType
fonts, and optional CJK or emoji font packs. Local paths, direct file output,
streaming, asynchronous conversion, and remote resources are intentionally
absent. They can be added later without changing existing symbols.
