#!/bin/bash


arch=$(uname -m)
os=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$os" in
    linux)
        target_triplet="${arch}-unknown-${os}-gnu"
        ;;
    darwin)
        target_triplet="${arch}-apple-darwin"
        ;;
    *)
        echo "Unsupported OS: $os"
        exit 1
        ;;
esac

latest_tag=$(curl -s https://api.github.com/repos/Vrtgs/hptp/releases/latest | jq -r .tag_name) || exit 1
wget "https://github.com/Vrtgs/hptp/releases/download/${latest_tag}/hptp-${target_triplet}.tar.gz" || exit 1
tar -xzvf "hptp-${target_triplet}.tar.gz" && rm "hptp-${target_triplet}.tar.gz"
