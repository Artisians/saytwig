#!/usr/bin/env python3
"""Shared, fail-fast local/CI gates. No automated tests, model downloads or device access."""
from pathlib import Path
import argparse
import ctypes
import os
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def run(*command):
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gate", choices=("format", "rust", "all"), default="all", nargs="?")
    parser.add_argument("--target", choices=("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc",
                                             "aarch64-apple-darwin", "x86_64-apple-darwin"))
    parser.add_argument("--profile", choices=("dev", "release"), default="dev")
    args = parser.parse_args()
    if os.name == "nt":
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.restype = ctypes.c_void_p
        kernel.IsWow64Process2.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ushort),
                                          ctypes.POINTER(ctypes.c_ushort)]
        process_machine = ctypes.c_ushort()
        native_machine = ctypes.c_ushort()
        if not kernel.IsWow64Process2(kernel.GetCurrentProcess(), ctypes.byref(process_machine),
                                      ctypes.byref(native_machine)):
            parser.error("Cannot establish the native Windows architecture")
        if native_machine.value != 0x8664 or struct.calcsize("P") != 8:
            parser.error("Windows gates require native x64 Windows and 64-bit Python, not ARM emulation")
        if args.target not in (None, "x86_64-pc-windows-msvc"):
            parser.error("Windows gates support only x86_64-pc-windows-msvc")
    elif args.target == "x86_64-pc-windows-msvc":
        parser.error("Windows gates require a native Windows x64 host")
    if args.gate in ("format", "all"):
        run("cargo", "fmt", "--all", "--", "--check")
    if args.gate in ("rust", "all"):
        # Windows is a native gate, not a cross-compile check: exercise tray and
        # platform code and ensure the actual app binary links.
        features = ["--features", "dictation-desktop/tray"] if os.name == "nt" else []
        target = ["--target", args.target] if args.target else []
        profile = ["--release"] if args.profile == "release" else []
        os.environ["CARGO_BUILD_WARNINGS"] = "deny"
        run("cargo", "clippy", "--workspace", "--lib", "--bins", "--locked", *target, *profile, *features, "--", "-D", "warnings")
        run("cargo", "build", "--locked", "--bin", "saytwig", *target, *profile, *features)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None
