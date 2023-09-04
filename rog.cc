#include <array>
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
using namespace std;
#define PORT 4433
#define CHECK(A,B) if ((A)<0){ perror(B); exit(1); }

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
	int len;
	string out;
	ringbuf rb{};
	thread t([&](){ sockserve(rb); });
	while (1){
		len = read(0, buf, sizeof(buf));
		rb.in(string_view(buf, len));
	}
	t.join();
}

void sockserve(ringbuf& rb){
	int sockfd;
    struct sockaddr_in serv_addr;
    char buffer[256];
    int n;
    
    CHECK(sockfd = socket(AF_INET, SOCK_STREAM, 0), "ERROR opening socket");
	serv_addr = {.sin_family=AF_INET, .sin_port=htons(PORT), .sin_addr={.s_addr=INADDR_ANY}};
    bzero(&(serv_addr.sin_zero), 8);
    CHECK(bind(sockfd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), ("ERROR on binding"));
    listen(sockfd,5);
    while(1){
        int new_socket;
        struct sockaddr_in cli_addr;
        socklen_t clilen = sizeof(cli_addr);
        CHECK(new_socket = accept(sockfd, (struct sockaddr *)&cli_addr, &clilen), "ERROR on accept");
		if ((n = read(new_socket, buffer, 255)) != 0) {
			buffer[n] = 0;
			printf("Received: %s\n", buffer);
			while (1){
				string log;
				if (!rb.out(log)){
					sleep(1);
					continue;
				}
				CHECK(sendto(new_socket, log.data(), log.size(), 0, (struct sockaddr *)&cli_addr, sizeof(cli_addr)), "Error sending data to client\n");
				printf("wrote %d\n", n);
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

    CHECK(sockfd = socket(AF_INET, SOCK_STREAM, 0), "Error creating socket\n");
    memset(&serv_addr, 0, sizeof(serv_addr));
    serv_addr.sin_family = AF_INET;
    serv_addr.sin_port = htons(PORT);
    inet_pton(AF_INET, "127.0.0.1", &serv_addr.sin_addr);
    CHECK(connect(sockfd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), "Error connecting to server\n");
    strcpy(buffer, "Hello from client!");
    CHECK(n = sendto(sockfd, buffer, strlen(buffer), 0, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), "Error sending data to server\n");
	while ((n = recvfrom(sockfd, buffer, sizeof(buffer), 0, NULL, NULL)) != 0){
		CHECK(n, "Error receiving data from server\n");
		buffer[n] = 0;
		printf("Received: %s\n", buffer);
	}
	printf("recieved %d, closing connection\n", n);
    close(sockfd);
}

int main(int argc, char** argv){
	if (argc < 2)
		readinput();
	else
		sockclient();
	return 0;
}
