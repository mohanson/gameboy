#!/usr/bin/env sh
set -xe
echo please enter a zip url for your rom
read url
wget "$url" -O "rom.zip"
unzip rom.zip
gameboy *.gb*
