#!/bin/sh
find /tmp -size 0 -name "data*" -exec rm -v {} \; 2>/dev/null
