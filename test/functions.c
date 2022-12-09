#include "test.h"

#include "curses.h"

int __wrap_key_get() {
    return -1;
}

// wrap with a plugin function
int_func_t key_poll_func = NULL;

int __wrap_key_poll() {
    if (key_poll_func != NULL) {
	return key_poll_func();
    }
    return -1;
}

int_func_t getch_func = NULL;

int __wrap_wgetch(WINDOW *w) {
    if (getch_func != NULL) {
	return getch_func();
    }
    return -1;
}

void __wrap_refreshp() {
}

int sendto_call_count = 0;
char *sendto_last_message = NULL;
int sendto_last_len = 0;

ssize_t __wrap_sendto(int sockfd, const void *buf, size_t len, int flags,
		      const struct sockaddr *dest_addr, socklen_t addrlen) {

    FREE_DYNAMIC_STRING(sendto_last_message);
    sendto_last_message = strdup(buf);
    sendto_last_len = len;
    ++sendto_call_count;

    return len;
}


const char STRING_NOT_SET[] = "__NOT_SET__";

const char *showmsg_spy = STRING_NOT_SET;
const char *showstring_spy1 = STRING_NOT_SET;
const char *showstring_spy2 = STRING_NOT_SET;

#define BUFSZ 200
char showmsg_spy_buf[BUFSZ];
char showstring_spy1_buf[BUFSZ];
char showstring_spy2_buf[BUFSZ];

static char *save_string(char *dest, const char *src) {
    strncpy(dest, src, BUFSZ - 1);
    dest[BUFSZ - 1] = 0;
    return dest;
}

void showmsg(char *message) {
    showmsg_spy = save_string(showmsg_spy_buf, message);
}

void showstring(char *message1, char *message2) {
    showstring_spy1 = save_string(showstring_spy1_buf, message1);
    showstring_spy2 = save_string(showstring_spy2_buf, message2);
}

unsigned int __wrap_sleep(unsigned int seconds) {
    // no sleeping in tests
    return 0;
}

int receive_packet(void) {
    return -1;
}

void rx_rtty() {
    return;
}

int fldigi_xmlrpc_get_carrier() {
    return -1;
}

int fldigi_get_log_call() {
    return -1;
}

int fldigi_get_log_serial_number() {
    return -1;
}

int fldigi_send_text(const char *line) {
    return -1;
}

extern bool fldigi_on;
bool fldigi_isenabled(void) {
    return fldigi_on;
}

void handle_lan_recv(int *lantimesync) {
}

void gettxinfo() {
}

void cqww_simulator(void) {
}

int hamlib_keyer_send(const char *cwmesage) {
    return -1;
}
