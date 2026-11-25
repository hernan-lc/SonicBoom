# Shared by the Termux entrypoints; no host toolchain settings are changed.
set -eu
: "${PREFIX:?Run inside Termux with PREFIX set}"
export PATH="$PREFIX/bin:$PATH"
fail() { echo "SonicBoom: $*" >&2; exit 1; }
[ "$(uname -m)" = aarch64 ] || fail 'Only Android ARM64 is supported'
# termux-docker shares the host kernel, so `uname -o` can report GNU/Linux
# there. Container use must be declared explicitly; the ORT ELF/digest pins
# and the aarch64-linux-android Rust host check still apply unchanged.
if [ "$(uname -o)" != Android ]; then
    [ "${SONICBOOM_TERMUX_CONTAINER:-0}" = 1 ] || fail 'Native Android/Termux is required'
fi
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
ORT_VERSION=1.24.3
ORT_SHA256=67397e4a970e75617f765d2015ceaf911917e1d822276cfb5792744e8085cbce
ORT_ARCHIVE_SHA256=$ORT_SHA256
ORT_LIBRARY_SHA256=4d2318b3849abb8862133d3068fc7e807ed8b2671cc6d83657fff2fcb9e1caad
ORT_CACHE=${XDG_CACHE_HOME:-$HOME/.cache}/sonicboom/runtime
export ORT_DYLIB_PATH=${ORT_DYLIB_PATH:-$PREFIX/lib/sonicboom/libonnxruntime.so}
validate_ort() {
    [ -f "$ORT_DYLIB_PATH" ] || fail "Missing $ORT_DYLIB_PATH; run scripts/termux/install.sh"
    if [ "$ORT_DYLIB_PATH" = "$PREFIX/lib/sonicboom/libonnxruntime.so" ] || [ "${SONICBOOM_ALLOW_CUSTOM_ORT:-0}" != 1 ]; then
        echo "$ORT_LIBRARY_SHA256  $ORT_DYLIB_PATH" | sha256sum -c - || fail 'ORT integrity check failed; custom paths require explicit SONICBOOM_ALLOW_CUSTOM_ORT=1 to use an unpinned runtime'
    fi
    readelf -h "$ORT_DYLIB_PATH" | grep -q 'Machine:.*AArch64' || fail 'ORT must be AArch64 ELF'
    readelf -h "$ORT_DYLIB_PATH" | grep -q 'Class:.*ELF64' || fail 'ORT must be ELF64'
    deps=$(readelf -d "$ORT_DYLIB_PATH")
    echo "$deps" | grep -q '\[libandroid.so\]' || fail 'ORT must be an Android/Bionic library'
    if echo "$deps" | grep -Eq 'libc.so.6|libstdc\+\+.so|ld-linux|libpthread.so|libdl.so.2'; then
        fail 'GNU/Linux runtime is incompatible with Android'
    fi
    readelf --dyn-syms --wide "$ORT_DYLIB_PATH" | grep -q 'OrtGetApiBase' || fail 'ORT C API missing'
}
# termux-docker's aosp-libs set lacks libandroid.so, so ORT's NEEDED entry
# cannot resolve there. Stage the libandroid-stub package copy beside the
# runtime, where its $ORIGIN runpath finds it; the stub's own runpath still
# resolves its deps from $PREFIX/lib. Real devices use /system/lib64 instead.
container_ort_deps() {
    [ "${SONICBOOM_TERMUX_CONTAINER:-0}" = 1 ] || return 0
    [ -f "$PREFIX/lib/libandroid.so" ] || fail 'Container mode needs libandroid-stub (pkg install libandroid-stub)'
    install -m 700 "$PREFIX/lib/libandroid.so" "$(dirname "$ORT_DYLIB_PATH")/libandroid.so"
}
prepare_ort() {
    if [ -f "$ORT_DYLIB_PATH" ]; then
        validate_ort
        container_ort_deps
        return
    fi
    mkdir -p "$ORT_CACHE"
    archive=$ORT_CACHE/onnxruntime-android-$ORT_VERSION.aar
    if [ ! -f "$archive" ]; then
        # Maven Central rate-limits (HTTP 429) some egress IPs for minutes.
        # The pinned digest makes a mirror safe: wrong bytes fail the check.
        for mirror in \
          "https://repo.maven.apache.org/maven2" \
          "https://maven-central.storage-download.googleapis.com/maven2"; do
            if curl --fail --location --retry 3 --retry-all-errors --proto '=https' \
              "$mirror/com/microsoft/onnxruntime/onnxruntime-android/$ORT_VERSION/onnxruntime-android-$ORT_VERSION.aar" \
              -o "$archive.part"; then
                break
            fi
        done
        echo "$ORT_ARCHIVE_SHA256  $archive.part" | sha256sum -c - || fail 'ORT archive integrity check failed'
        mv "$archive.part" "$archive"
    fi
    echo "$ORT_ARCHIVE_SHA256  $archive" | sha256sum -c - || fail 'ORT cache integrity check failed; remove the archive and retry'
    staging=$(mktemp -d)
    unzip -p "$archive" jni/arm64-v8a/libonnxruntime.so > "$staging/libonnxruntime.so"
    destination=$ORT_DYLIB_PATH
    # `VAR=x validate_ort` leaks past function calls on some shells (aosp mksh
    # in termux-docker): set and restore explicitly so the final check looks
    # at the installed path, not the deleted staging copy.
    ORT_DYLIB_PATH=$staging/libonnxruntime.so
    validate_ort
    ORT_DYLIB_PATH=$destination
    mkdir -p "$(dirname "$destination")"
    if [ -e "$destination" ] && ! cmp -s "$staging/libonnxruntime.so" "$destination"; then
        rm -rf "$staging"
        fail "Refusing to replace a different runtime at $destination"
    fi
    if [ ! -e "$destination" ]; then
        install -m 700 "$staging/libonnxruntime.so" "$destination.tmp"
        mv "$destination.tmp" "$destination"
    fi
    rm -rf "$staging"
    validate_ort
    container_ort_deps
}
