#include <arpa/inet.h>
#include <bits/getopt_core.h>
#include <netdb.h>
#include <netinet/in.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <unistd.h>
#include <algorithm>
#include <array>
#include <iostream>
#include <iterator>
#include <mutex>
#include <regex>
#include <thread>
using namespace std;
#define PORT 4433
#define CHECK(A,B) if ((A)<0){ perror(B); globals::running = false; exit(1); }

namespace globals {
bool running = true;
bool follow = true;
int amount = 0;
}

class ipcmsg {
	public:
	enum {
		LOOP = 0,
		SINGLE = 1,
	};
	int what;
};

class ringbuf {
	static const unsigned long bufsize = 1024;
	std::array<char, bufsize+1> buf{0};
	mutex mtx;
	int front = 0;
	int rear = 0;
	bool hasdata = 0;
	public:
	ringbuf(){
		fill(buf.begin(), buf.end(), 0);
		mtx.lock();
	}
	void in(string_view data){
		if (data.size() == 0)
			return;
		if (data.size() > bufsize)
			data = data.substr(data.size() - bufsize);
		auto writeamount = min(data.size(), bufsize - front);
		copy(data.begin(), data.begin()+writeamount, buf.begin()+front);
		if (writeamount == data.size()){
			int nextfront = front + writeamount;
			if (rear > front && rear < nextfront)
				rear = nextfront;
			front = nextfront;
			hasdata = true;
			mtx.unlock();
			return;
		}
		copy(data.begin()+writeamount, data.end(), buf.begin());
		int nextfront = data.size() - writeamount;
		if (rear < nextfront)
			rear = nextfront;
		front = nextfront;
		hasdata = true;
		mtx.unlock();
	}
	void out(string& out){
		out.clear();
		if (!hasdata)
			mtx.lock();
		if (rear < front){
			out.insert(out.begin(), buf.begin()+rear, buf.begin()+front);
		} else {
			out.insert(out.begin(), buf.begin()+rear, buf.end()-1);
			out.insert(out.end(), buf.begin(), buf.begin()+front);
		}
		rear = front;
		hasdata = false;
	}
};

void sockserve(ringbuf& rb);
void readinput(){
	char buf[1024];
	memset(buf, 0, sizeof(buf));
	int i;
	ringbuf rb{};
	thread serverThread([&](){ sockserve(rb); });
	while (globals::running){
		i = read(0, buf, sizeof(buf)-1);
		rb.in(string_view(buf, i));
		buf[0]=0;
	}
	serverThread.join();
}

void sockserve(ringbuf& rb){
	int sockfd;
	struct sockaddr_in serv_addr;
	CHECK(sockfd = socket(AF_INET, SOCK_STREAM, 0), "ERROR opening socket");
	serv_addr = {.sin_family=AF_INET, .sin_port=htons(PORT), .sin_addr={.s_addr=INADDR_ANY}};
	CHECK(bind(sockfd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), "ERROR binding");
	CHECK(listen(sockfd,5), "Error listening");
	while(1){
		int new_socket;
		struct sockaddr_in cli_addr;
		socklen_t clilen = sizeof(cli_addr);
		CHECK(new_socket = accept(sockfd, (struct sockaddr *)&cli_addr, &clilen), "ERROR on accept");
		ipcmsg command{0};
		string data;
		CHECK(read(new_socket, &command, sizeof(ipcmsg)), "Error reading control message");
		switch (command.what){
		case ipcmsg::LOOP:{
			while (1){
				rb.out(data);
				if (write(new_socket, data.data(), data.size()) < 0){
					break;
				}
			}
			break;}
		case ipcmsg::SINGLE:{
			rb.out(data);
			write(new_socket, data.data(), data.size());
			break;}
		}
		close(new_socket);
	}
}



struct in_addr getAddress(char* host){
	struct in_addr ret;

	// first see if given ip address
	regex ippat("[0-9]{1,3}.[0-9]{1,3}.[0-9]{1,3}.[0-9]{1,3}");
	if(regex_match(host, ippat)){
		inet_pton(AF_INET, host, &ret);
		return ret;
	}

	// otherwise get ip from hostname
	struct addrinfo hints, *result;
	memset(&hints, 0, sizeof(struct addrinfo));
	hints.ai_family = AF_INET;
	hints.ai_socktype = SOCK_STREAM;
	hints.ai_protocol = IPPROTO_TCP;
	if (getaddrinfo(host, NULL, &hints, &result)){
		cerr << "Error: Unable to resolve hostname" << endl;
		exit(1);
	}
	char ip_address[INET_ADDRSTRLEN];
	ret = ((struct sockaddr_in *)result->ai_addr)->sin_addr;
	auto fam = result->ai_family;
	freeaddrinfo(result);
	inet_ntop(fam, &ret, ip_address, INET_ADDRSTRLEN);
	return ret;
}

void sockclient(struct in_addr remotehost){
	int sockfd;
	struct sockaddr_in serv_addr;
	char buffer[1024];
	int n;
	CHECK(sockfd = socket(AF_INET, SOCK_STREAM, 0), "Error creating socket");
	memset(&serv_addr, 0, sizeof(serv_addr));
	serv_addr.sin_family = AF_INET;
	serv_addr.sin_port = htons(PORT);
	serv_addr.sin_addr = remotehost;
	CHECK(connect(sockfd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), "Error connecting to server");
	ipcmsg msg{ .what = globals::follow ? ipcmsg::LOOP : ipcmsg::SINGLE };
	CHECK(write(sockfd, &msg, sizeof(msg)), "Error sending control message");
	while ((n = recvfrom(sockfd, buffer, sizeof(buffer), 0, NULL, NULL)) != 0){
		CHECK(n, "Error receiving data from server\n");
		buffer[n] = 0;
		cout << buffer;
	}
	close(sockfd);
}


int main(int argc, char** argv){
	sigset_t set;
	sigaddset(&set, SIGPIPE);
	CHECK(sigprocmask(SIG_BLOCK, &set, NULL), "Block broken pipe signal");
	if (argc < 2) {
		readinput();
		return 0;
	}

	auto host = (char*)"localhost";
	for(char c; (c = getopt(argc, argv, "ti:c:")) != -1;)
		switch(c){
		case 'i':
			host = optarg;
			break;
		case 'c':
			globals::amount = atoi(optarg);
		case 't':
			globals::follow = false;
			break;
		};
	sockclient(getAddress(host));
	return 0;
}
