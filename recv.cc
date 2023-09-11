#include <array>
#include <sys/poll.h>
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
#include <poll.h>
using namespace std;
#define PORT 4433
#define CHECK(A,B) if ((A)<0){ perror(B); exit(1); }

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
	while ((n = recvfrom(sockfd, buffer, sizeof(buffer), 0, NULL, NULL)) != 0){
		CHECK(n, "Error receiving data from server\n");
		buffer[n] = 0;
		printf("Received: %s\n", buffer);
	}
	printf("recieved %d, closing connection\n", n);
    close(sockfd);
}

int main(int argc, char** argv){
	sockclient();
	return 0;
}
