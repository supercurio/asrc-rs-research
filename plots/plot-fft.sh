#!/usr/bin/gnuplot -p
set grid x y

plot "/tmp/fft.dat" using 1:2 title "fft" with lines,

pause -1
