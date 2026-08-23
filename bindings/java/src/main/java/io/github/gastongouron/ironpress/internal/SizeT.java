package io.github.gastongouron.ironpress.internal;

import com.sun.jna.IntegerType;
import com.sun.jna.Native;

/** JNA value with the platform width of C {@code size_t}. */
public final class SizeT extends IntegerType {
  private static final long serialVersionUID = 1L;

  /** Create a zero value for JNA reflection. */
  public SizeT() {
    this(0);
  }

  /** Create an unsigned platform-width size. */
  public SizeT(long value) {
    super(Native.SIZE_T_SIZE, value, true);
  }
}
