FROM rayproject/ray-deps:latest
RUN sudo apt-get update && \
    sudo apt-get install -y vim curl python3 python3-pip \
    unzip cmake gcc g++ llvm tree libclang-dev npm && \
    sudo apt-get clean && \
    sudo ln -s /usr/bin/python3 /usr/bin/python && \
    sudo ln -s /usr/bin/python3 /usr/local/bin/python
RUN wget https://github.com/bazelbuild/bazelisk/releases/download/v1.16.0/bazelisk-linux-amd64
RUN sudo chmod +x bazelisk-linux-amd64 && sudo mv bazelisk-linux-amd64 /usr/bin/bazel
RUN curl -sSf https://raw.githubusercontent.com/WasmEdge/WasmEdge/master/utils/install.sh | sudo bash -s -- \
    --plugins wasi_nn-tensorflowlite wasmedge_tensorflow wasi_nn-openvino wasi_nn-pytorch -p /usr/local

WORKDIR /usr/local/lib
ENV PYTORCH_VERSION="1.8.2"
# For the Ubuntu 20.04 or above, use the libtorch with cxx11 abi.
ENV PYTORCH_ABI="libtorch-cxx11-abi"
# For the manylinux2014, please use the without cxx11 abi version:
#   export PYTORCH_ABI="libtorch"
RUN sudo curl -s -L -O --remote-name-all https://download.pytorch.org/libtorch/lts/1.8/cpu/${PYTORCH_ABI}-shared-with-deps-${PYTORCH_VERSION}%2Bcpu.zip
RUN sudo unzip -q "${PYTORCH_ABI}-shared-with-deps-${PYTORCH_VERSION}%2Bcpu.zip"
RUN sudo rm -f "${PYTORCH_ABI}-shared-with-deps-${PYTORCH_VERSION}%2Bcpu.zip"

# put libtorch into the LD_LIBRARY_PATH in the profile
RUN echo "export LD_LIBRARY_PATH=$(pwd)/libtorch/lib:\${LD_LIBRARY_PATH}" | sudo tee -a /etc/profile
WORKDIR /
