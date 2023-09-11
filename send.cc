#include <array>
#include <bits/types/struct_timespec.h>
#include <ctime>
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
#include <signal.h>
using namespace std;
#define PORT 4433
#define CHECK(A,B) if ((A)<0){ perror(B); exit(1); }

void sockserve(){
	sigset_t set;
    sigaddset(&set, SIGPIPE);
    int retcode = sigprocmask(SIG_BLOCK, &set, NULL);
    if (retcode == -1) perror("sigprocmask");
	int sockfd, n;
    struct sockaddr_in serv_addr;
	struct pollfd fds[1];
    char buffer[256];
    CHECK(sockfd = socket(AF_INET, SOCK_STREAM, 0), "ERROR opening socket");
	serv_addr = {.sin_family=AF_INET, .sin_port=htons(PORT), .sin_addr={.s_addr=INADDR_ANY}};
    CHECK(bind(sockfd, (struct sockaddr *)&serv_addr, sizeof(serv_addr)), ("ERROR on binding"));
    listen(sockfd,5);
    while(1){
        int new_socket;
        struct sockaddr_in cli_addr;
        socklen_t clilen = sizeof(cli_addr);
        CHECK(new_socket = accept(sockfd, (struct sockaddr *)&cli_addr, &clilen), "ERROR on accept");
		while (1){
			time_t now = time(0);
			struct tm *local = localtime(&now);
			char timebuf[80];
			strftime(timebuf, sizeof(timebuf), "%c", local);
			CHECK(n = sendto(new_socket, timebuf, strlen(timebuf), 0, (struct sockaddr *)&cli_addr, sizeof(cli_addr)), "Error sending data to client\n");
			sleep(1);
		}
		close(new_socket);
	}
}

int main(int argc, char** argv){
	sockserve();
	return 0;
}
