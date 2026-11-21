#!/usr/bin/env bash
BUILD_HOST=${AY_BUILD_HOST:-build}
TEST_HOST=${AY_TEST_HOST:-test}
PROD_HOST=${AY_PROD_HOST:-prod}
# Usage: run from a machine with ssh access to build/test/prod hosts. No Chromium build is started.
set -euo pipefail
# CHROMIUM_OUT: the Chromium out directory on the build host (<src>/out/<name>); siso comes with the checkout.
: "${CHROMIUM_OUT:?set CHROMIUM_OUT to the Chromium out directory on the build host}"
OUT=$CHROMIUM_OUT
SISO=${SISO:-$OUT/../../third_party/siso/cipd/siso}
here=$(cd "$(dirname "$0")" && pwd)
ssh "$BUILD_HOST" "set -e; mkdir -p /tmp/bm; cd $OUT; nice -n 19 $SISO query graph -C . | nice -n 19 gzip -1 > /tmp/bm/graph.jsonl.gz;
  cp siso_metrics.0.json siso_metrics.json /tmp/bm/; cp ../re-test/siso_metrics.json /tmp/bm/retest_metrics.json"
ssh "$PROD_HOST" "set -e; mkdir -p ~/bm/data; cd ~/bm/data; ssh \"${BUILD_HOST}-wireguard\" \"cd /tmp/bm && tar cf - .\" | tar xf -
  zcat graph.jsonl.gz > graph.jsonl
  cd \"\${FW_DIR:-\$HOME/src/fw}\" && nice cargo build --release -p fw-depot --bin fw-depot
  cd ~/bm && nice \"\${FW_DIR:-\$HOME/src/fw}/target/release/fw-depot\" model --graph data/graph.jsonl \\
    --run1 data/siso_metrics.0.json --run2 data/siso_metrics.json --retest data/retest_metrics.json \\
    --pool \"mid=build:14:0.857:0,prod:4:1.077:0.025,test:3:0.819:0.025\" \\
    --pool \"opt=build:14:0.857:0,prod:4:1.612:0.025,test:3:1.226:0.025\" \\
    --pool \"coldcas=build:14:0.857:0,prod:4:1.077:11.07,test:3:0.819:11.07\" \\
    --boot 1000 --link 600,1800,3600
  rm -f data/graph.jsonl"
ssh "$BUILD_HOST" 'rm -rf /tmp/bm'
