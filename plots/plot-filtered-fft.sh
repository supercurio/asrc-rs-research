#!/usr/bin/gnuplot -p
set grid x y

plot \
	    "/tmp/filtered-fft-1.dat" using 1:2 title "fft filtered 1" with lines, \
	    "/tmp/filtered-fft-2.dat" using 1:2 title "fft filtered 2" with lines

pause -1