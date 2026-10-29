# Bundled speech dependencies

This is an independent application integration, not an endorsement by Fermion
Research, NVIDIA, Python, PyTorch or Hugging Face. The integration scripts are
newly authored application code. Bundled third-party code and weights remain
upstream work under their respective licenses.

- **Fermion Research 0.2.15:** Apache-2.0. The upstream wheel's LICENSE and NOTICE
  are retained in `python/Lib/site-packages/fermion_research-0.2.15.dist-info/licenses/`.
  Source: https://pypi.org/project/fermion-research/0.2.15/
- **Phonon-2.5 English weights:** CC-BY-4.0, derived from NVIDIA's
  `parakeet-tdt-0.6b-v3`. The pinned upstream NOTICE, README, weights license and
  code license are preserved under `notices/MODEL-*`. We reconstruct the exact
  published container without changing its weights. The bundle contains no
  model weights; downloading them is an explicit application operation.
  Source: https://huggingface.co/FermionResearch/Phonon-2.5-English/tree/da200358d638886bc96c758c672ad5568828b119
- **Python 3.13.16:** PSF License Agreement and included third-party terms,
  retained in `python/LICENSE.txt`.
  Source: https://www.python.org/downloads/release/python-31316/
- **PyTorch and other Python dependencies:** the original wheel metadata,
  licenses and notices are retained. `assets.json` identifies exact upstream
  versions, download URLs and SHA-256 hashes. PyTorch's license contains its
  bundled component notices. The tokenizers 0.23.2 Apache-2.0 license is also
  provided separately as `notices/TOKENIZERS-LICENSE`.

Only PyTorch's `torch/lib/*.lib` Windows C++ linker inputs are omitted. This
runtime uses the upstream prebuilt, hash-verified DLL kernels and does not build
C++ extensions. No DLL, Python module, header, metadata file or license is removed
by that rule. `bundle.json` records the original omitted-file hashes and sizes.
Manual verification of the packaged Windows ABI and real speech recognition remains necessary before a
release; assembling these assets on Linux does not establish Windows execution.
