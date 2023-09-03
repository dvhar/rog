#include <array>
#include <iostream>
#include <algorithm>
#include <stdexcept>
using namespace std;

class ringbuf {
	static const unsigned long bufsize = 1024;
	std::array<char, bufsize+1> buf{0};
	int front = 0;
	int rear = 0;
	bool hasdata = 0;
	public:
	ringbuf(){
		fill(buf.begin(), buf.end(), 0);
	}
	void write(string_view data){
		if (data.size() == 0)
			return;
		hasdata = true;
		if (data.size() > bufsize)
			data = data.substr(data.size() - bufsize);
		auto writeamount = min(data.size(), bufsize - front);
		copy(data.begin(), data.begin()+writeamount, buf.begin()+front);
		if (writeamount == data.size()){
			int nextfront = front + writeamount;
			if (rear > front && rear < nextfront)
				rear = nextfront;
			front = nextfront;
			return;
		}
		copy(data.begin()+writeamount, data.end(), buf.begin());
		int nextfront = data.size() - writeamount;
		if (rear < nextfront)
			rear = nextfront;
		front = nextfront;
	}
	int read(string& out){
		out.clear();
		if (!hasdata)
			return 0;
		if (rear < front){
			out.insert(out.begin(), buf.begin()+rear, buf.begin()+front);
		} else {
			out.insert(out.begin(), buf.begin()+rear, buf.end()-1);
			out.insert(out.end(), buf.begin(), buf.begin()+front);
		}
		rear = front;
		hasdata = false;
		return out.size();
	}
};

int main(int argc, char** argv){
	ringbuf rb{};
	return 0;
}
