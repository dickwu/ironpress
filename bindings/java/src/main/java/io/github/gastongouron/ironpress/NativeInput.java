package io.github.gastongouron.ironpress;

import com.sun.jna.Memory;
import com.sun.jna.Pointer;
import io.github.gastongouron.ironpress.internal.NativeBytes;
import java.nio.ByteBuffer;
import java.nio.CharBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.Objects;

/** Deterministically owned native copy of one borrowed ABI input. */
final class NativeInput implements AutoCloseable {
  private final Memory memory;
  private final NativeBytes bytes;

  private NativeInput(Memory memory, NativeBytes bytes) {
    this.memory = memory;
    this.bytes = bytes;
  }

  static NativeInput text(String value, String parameterName) {
    Objects.requireNonNull(value, parameterName);
    var encoder =
        StandardCharsets.UTF_8
            .newEncoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT);
    try {
      return binary(toByteArray(encoder.encode(CharBuffer.wrap(value))), parameterName);
    } catch (CharacterCodingException error) {
      throw new IllegalArgumentException(
          parameterName + " must contain valid Unicode scalar values.", error);
    }
  }

  static NativeInput binary(byte[] value, String parameterName) {
    Objects.requireNonNull(value, parameterName);
    if (value.length == 0) {
      return new NativeInput(null, new NativeBytes(Pointer.NULL, 0));
    }

    var memory = new Memory(value.length);
    memory.write(0, value, 0, value.length);
    return new NativeInput(memory, new NativeBytes(memory, value.length));
  }

  NativeBytes bytes() {
    return bytes;
  }

  @Override
  public void close() {
    if (memory != null) {
      memory.close();
    }
  }

  private static byte[] toByteArray(ByteBuffer buffer) {
    var bytes = new byte[buffer.remaining()];
    buffer.get(bytes);
    return bytes;
  }
}
