#!/usr/bin/gnuplot
set grid x y

plot \
	    "/tmp/filtered-1.dat" title "filtered data 1" with lines, \
	    "/tmp/filtered-2.dat" title "filtered data 2" with lines

pause -1
