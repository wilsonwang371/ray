FROM rayproject/ray-deps:latest
RUN sudo apt-get update && sudo apt-get install -y vim curl python3 python3-pip unzip cmake gcc g++ llvm tree libclang-dev npm && sudo apt-get clean && sudo ln -s /usr/bin/python3 /usr/bin/python && sudo ln -s /usr/bin/python3 /usr/local/bin/python
RUN wget https://github.com/bazelbuild/bazelisk/releases/download/v1.16.0/bazelisk-linux-amd64
RUN sudo chmod +x bazelisk-linux-amd64 && sudo mv bazelisk-linux-amd64 /usr/bin/bazel
