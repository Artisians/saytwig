# Windows x64 development speech runtime

Saytwig is a local speech recognition application. This document describes its
technical speech runtime, not a signed installer or a consumer-ready download.

## Target contract

- Windows **x64 only**: native AMD64 Windows with an x64 app/interpreter.
- No Windows ARM64/ARM32/x86 artifact, updater target, or emulation support is
  offered. Running the x64 executable under Windows-on-ARM emulation is rejected.
- Rust rejects non-x64 Windows builds at compilation. Before runtime verification
  or installation, it checks the **native** host machine with
  [`IsWow64Process2`](https://learn.microsoft.com/en-us/windows/win32/api/wow64apiset/nf-wow64apiset-iswow64process2).
  The standalone bridge and model reconstruction command use the same guard.
  These API calls require Windows 10 version 1709 or later; that API floor is not
  a claim of hands-on compatibility with every Windows version after it.
- `assets.json` and generated `bundle.json` declare `windows-x86_64`. The assembler
  rejects a non-AMD64 Python embed archive or native wheel tags other than
  `win_amd64`; portable `any` wheels remain valid. The Windows runtime refuses
  older manifests without this target declaration rather than guessing.
- This restriction concerns Windows only. It does not change other platforms'
  architecture policy. Linux cloud build/research tools may assemble Windows x64
  inputs but cannot establish that those binaries execute successfully.

## Exact inputs and output

The immutable `assets.json` lock contains CPython 3.13.16 AMD64, Fermion 0.2.15,
42 Python distributions and independently pinned upstream notices. The model
archive remains a separate explicit download: 157,616,598 compressed bytes;
177,757,399 bytes for its three reconstructed files. No other models are fetched.

Build a **new** development output directory using the existing production
assembler; it refuses to overwrite an existing destination:

```powershell
python -B scripts/phonon/assemble.py --cache C:\build\speech-cache --output C:\build\saytwig-speech-windows-x64
```

Keep the entire output, including `notices/`, `python/LICENSE.txt`, wheel metadata,
licenses, `assets.json` and `bundle.json`. `notices/DEPENDENCIES.json` indexes full
retained texts; see [redistribution qualifications](NOTICES.md). The generated
manifest digest is a development integrity value. It is **not** a signature and
must not become trusted merely because it sits beside an untrusted bundle.
Production distribution still needs reviewed, independently trusted metadata.

Only 19 PyTorch `.lib` link inputs are omitted (859,703,748 bytes), not DLLs,
headers, Python modules, metadata or notices. The approximately 665 MB unpacked
runtime is separate from the app executable and model. Source/notice changes
alter its final file inventory and manifest hash; do not reuse a historical hash
for a regenerated bundle. Do not label this directory an MSI/EXE installer.

## Native launch prerequisites and development setup

- Native Windows x64 and the normal native desktop/audio environment are required
  for actual application operation. Cloud Linux observations are not Windows QA.
- A suitable Microsoft Visual C++ **x64** runtime must already be present,
  including dependencies such as `msvcp140.dll`. The Python embed archive alone
  does not provide the complete vendor dependency set. Determine the required
  toolset version from the actual native build; use Microsoft's signed official
  distribution under its terms. Do not copy arbitrary DLLs from a development
  machine or silently install system prerequisites. No such installation has
  been performed by this work.
- Use the bundle's embedded interpreter, not a global Python/pip install. Its
  `._pth` file intentionally omits `import site`; do not loosen isolation to make
  an import succeed. Model reconstruction and inference operate offline after
  explicit provisioning, with the pinned local files.
- For a reviewed local development build, supply the absolute runtime directory,
  its independently known digest and a model-store directory to the existing app:

```powershell
$env:SAYTWIG_RUNTIME_DIR = 'C:\build\saytwig-speech-windows-x64'
$env:SAYTWIG_RUNTIME_SHA256 = '<independently reviewed development inventory digest>'
$env:SAYTWIG_MODEL_STORE = "$env:LOCALAPPDATA\Saytwig\models"
.\saytwig.exe
```

The app does not implicitly download a missing runtime. The model install action
is explicit and refuses an unrecognized nonempty store or unknown existing model
files. A supplied preinstalled model must be the pinned three-file installation
under `phonon-2.5-en`. App and runtime architecture checks do not replace a real
first-run/upgrade/permission/inference inspection on the target machine.

## What remains unverified or intentionally disabled

No native Windows speech execution, ABI/import result, microphone capture,
Windows-on-ARM rejection observation, signed installer or complete runtime
redistribution approval is claimed. Cross-compilation validates source shape,
not those behaviors. Nightly/stable branch naming does not turn a development
artifact into a supported release. Keep native MSVC builds and manual Windows
operation as review gates; publication is owned by the release coordinator.
