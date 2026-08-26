#!/usr/bin/bash

# inotify tools is required

tree-sitter generate
tree-sitter build --wasm
tree-sitter playground --quiet &
echo $! > ./pid

while inotifywait -e modify ./grammar.js; do
    echo "Grammar changed, restarting"
    kill -SIGINT $(cat ./pid)
    pkill tree-sitter
    sleep 2
    tree-sitter generate
    tree-sitter build --wasm
    tree-sitter playground --quiet &
    echo $! > ./pid
done