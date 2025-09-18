#!/bin/bash

# Set ROCm environment variables
export HSA_OVERRIDE_GFX_VERSION='11.0.0'
export HIP_VISIBLE_DEVICES='0'
export CUDA_VISIBLE_DEVICES='0'

# Set library path for ROCm CTranslate2
HOME_DIR="$HOME"
CTRANSLATE2_LIB_PATH="$HOME_DIR/repos/dictation/CTranslate2-rocm/CTranslate2-rocm/build"
export LD_LIBRARY_PATH="$CTRANSLATE2_LIB_PATH:$HOME_DIR/.local/lib64:$HOME_DIR/.local/lib:$LD_LIBRARY_PATH"

# Run the daemon
exec ./venv/bin/python dictation_daemon.py "$@"