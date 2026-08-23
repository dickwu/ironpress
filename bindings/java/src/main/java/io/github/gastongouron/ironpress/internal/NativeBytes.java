package io.github.gastongouron.ironpress.internal;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

/** Borrowed pointer-and-length input passed by value through the C ABI. */
@Structure.FieldOrder({"data", "length"})
public final class NativeBytes extends Structure implements Structure.ByValue {
  /** First borrowed byte, or null for an empty input. */
  public Pointer data;

  /** Number of readable borrowed bytes. */
  public SizeT length;

  /** Create an empty value for JNA reflection. */
  public NativeBytes() {
    this(Pointer.NULL, 0);
  }

  /** Create a borrowed byte range. */
  public NativeBytes(Pointer data, long length) {
    this.data = data;
    this.length = new SizeT(length);
  }
}
