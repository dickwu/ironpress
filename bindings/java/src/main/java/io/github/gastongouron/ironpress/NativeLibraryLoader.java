package io.github.gastongouron.ironpress;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Platform;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.Set;

/** Loads and verifies the packaged native library before it owns any state. */
final class NativeLibraryLoader {
  private static final Set<String> SUPPORTED_PREFIXES =
      Set.of("linux-x86-64", "linux-aarch64", "darwin-x86-64", "darwin-aarch64", "win32-x86-64");

  private static final NativeApi API = load();
  private static final String NATIVE_VERSION = readNativeVersion(API);

  private NativeLibraryLoader() {}

  static NativeApi api() {
    return API;
  }

  static String nativeVersion() {
    return NATIVE_VERSION;
  }

  private static NativeApi load() {
    if (!SUPPORTED_PREFIXES.contains(Platform.RESOURCE_PREFIX)) {
      throw new UnsupportedOperationException(
          "Ironpress does not ship a native asset for " + Platform.RESOURCE_PREFIX + ".");
    }

    var options =
        Map.<String, Object>of(
            Library.OPTION_CLASSLOADER, NativeLibraryLoader.class.getClassLoader());
    var api = Native.load("ironpress_ffi", NativeApi.class, options);
    var abiVersion = api.ironpress_abi_version();
    if (abiVersion != IronpressInfo.REQUIRED_ABI_VERSION) {
      throw new LinkageError(
          "Ironpress ABI "
              + IronpressInfo.REQUIRED_ABI_VERSION
              + " is required, but ABI "
              + abiVersion
              + " was loaded.");
    }

    var nativeVersion = readNativeVersion(api);
    if (!IronpressInfo.PACKAGE_VERSION.equals(nativeVersion)) {
      throw new LinkageError(
          "Ironpress Java "
              + IronpressInfo.PACKAGE_VERSION
              + " loaded native library "
              + nativeVersion
              + ".");
    }
    return api;
  }

  private static String readNativeVersion(NativeApi api) {
    var version = api.ironpress_version();
    if (version == null) {
      throw new LinkageError("The Ironpress native library returned no package version.");
    }
    return version.getString(0, StandardCharsets.UTF_8.name());
  }
}
