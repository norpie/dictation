# Python Virtual Environment Setup for ROCm

## Critical Setup Order (DO NOT DEVIATE)

1. **Install ROCm requirements FIRST**
   ```bash
   daemon-py/venv/bin/pip install -r daemon-py/requirements-rocm.txt
   ```

2. **Install normal requirements SECOND**
   ```bash
   daemon-py/venv/bin/pip install -r daemon-py/requirements.txt
   ```

3. **Install our custom ctranslate2 library to override THIRD**
   ```bash
   # Remove the pip-installed ctranslate2
   rm -rf daemon-py/venv/lib/python3.11/site-packages/ctranslate2*

   # Copy our custom ROCm-enabled build (now built for Python 3.11)
   cp -r python/build/lib.linux-x86_64-cpython-311/ctranslate2 daemon-py/venv/lib/python3.11/site-packages/
   ```

## Environment Requirements

The daemon MUST be run with these environment variables:
```bash
export LD_LIBRARY_PATH="/home/norpie/.local/lib64:/home/norpie/.local/lib:$LD_LIBRARY_PATH"
export HSA_OVERRIDE_GFX_VERSION='11.0.0'
export HIP_VISIBLE_DEVICES='0'
export CUDA_VISIBLE_DEVICES='0'
```

## Status

- [x] Custom ctranslate2 built and installed for Python 3.11 with ROCm support
- [x] Library path updated to use installed location (~/.local/lib64)
- Use the wrapper script `run_daemon.sh` to ensure proper environment setup

## DO NOT

- Change the order of installation
- Install torch/torchaudio separately without using requirements-rocm.txt
- Try to install ctranslate2 via pip after copying our custom build
- Delete the venv and start over unless you follow this exact order