#include <array>
#include <iterator>
#include <thread>
#include <iostream>
#include <algorithm>
#include <stdexcept>
#include <unistd.h>
#include <sys/types.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
using namespace std;
#define PORT 4433
#define CHECK(A,B) if ((A)<0){ perror(B); running = false; exit(1); }
bool running = true;

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
	void in(string_view data){
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
	int out(string& out){
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

void sockserve(ringbuf& rb);
void readinput(){
	char buf[1024];
	memset(buf, 0, sizeof(buf));
	int i;
	ringbuf rb{};
	thread t([&](){ sockserve(rb); });
	while (running){
		i = read(0, buf, sizeof(buf)-1);
		rb.in(string_view(buf, i));
		buf[0]=0;
	}
	t.join();
}

void sockserve(ringbuf& rb){
	int sockfd;
    struct sockaddr_in serv_addr;
    CHECK(sockfd = socket(AF_INET, SOCK_STREAM, 0), "ERROR opening socket");
	serv_addr = {.sin_family=AF_INET, .sin_port=htons(PORT), .sin_addr={.s_addr=INADDR_ANY}};
    CHECK(bind(sockfd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), "ERROR binding");
    listen(sockfd,5);
    while(1){
        int new_socket;
        struct sockaddr_in cli_addr;
        socklen_t clilen = sizeof(cli_addr);
        CHECK(new_socket = accept(sockfd, (struct sockaddr *)&cli_addr, &clilen), "ERROR on accept");
		while (1){
			string log;
			if (!rb.out(log)){
				sleep(1);
				continue;
			}
			if (sendto(new_socket, log.data(), log.size(), 0, (struct sockaddr *)&cli_addr, sizeof(cli_addr)) < 0){
				break;
			}
		}
		close(new_socket);
	}
}


void sockclient(){
	int sockfd;
    struct sockaddr_in serv_addr;
    char buffer[1024];
    int n;

    CHECK(sockfd = socket(AF_INET, SOCK_STREAM, 0), "Error creating socket");
    memset(&serv_addr, 0, sizeof(serv_addr));
    serv_addr.sin_family = AF_INET;
    serv_addr.sin_port = htons(PORT);
    inet_pton(AF_INET, "127.0.0.1", &serv_addr.sin_addr);
    CHECK(connect(sockfd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), "Error connecting to server");
	while ((n = recvfrom(sockfd, buffer, sizeof(buffer), 0, NULL, NULL)) != 0){
		CHECK(n, "Error receiving data from server\n");
		buffer[n] = 0;
		cout << buffer;
	}
	printf("recieved %d, closing connection\n", n);
    close(sockfd);
}

int main(int argc, char** argv){
	sigset_t set;
    sigaddset(&set, SIGPIPE);
    CHECK(sigprocmask(SIG_BLOCK, &set, NULL), "Block broken pipe signal");
	if (argc < 2)
		readinput();
	else
		sockclient();
	return 0;
}
