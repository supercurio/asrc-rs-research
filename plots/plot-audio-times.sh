#!/usr/bin/gnuplot
set grid x y

plot \
	    "/tmp/dump.dat" using 1:3 title "fixed audio rate" with lines, \
	    "/tmp/dump.dat" using 1:4 title "calculated" with lines, \

#plot \
#	    "/tmp/dump.dat" using 1:3 title "audio rate" with lines, \
#	    "/tmp/dump.dat" using 1:4 title "system rate" with lines, \

#plot \
#	    "/tmp/dump.dat" using 1:5 title "audio rate / system rate" with lines, \
#	    "/tmp/dump.dat" using 1:6 title "48000 / system rate" with lines, \


#plot \
#	    "/tmp/dump.dat" using 1:7 title "drift" with lines, \

pause -1
