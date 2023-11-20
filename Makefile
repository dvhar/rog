CC = g++
CFLAGS = -Wall -O3 --std=gnu++17
TARGET = rog
SOURCES = $(wildcard *.cc)
all: $(TARGET)
$(TARGET): $(SOURCES)
	$(CC) -o $@ $(CFLAGS) $(SOURCES)
clean:
	rm -f *.o *~ core $(TARGET)
