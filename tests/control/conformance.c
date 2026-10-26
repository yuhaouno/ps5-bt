/* SPDX-License-Identifier: GPL-3.0-or-later */
#define PS5_BT_LISTENER_TEST
#include "../../platform/control/tcp_listener.c"
#include <assert.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
static unsigned fault_stage,fault_nth,fault_calls,close_calls,close_interrupt,short_io,dirty_recv,dirty_send,read_calls,write_calls;
static int fault_errno=EIO;
int ps5_bt_listener_test_fault(uint32_t stage) {
    if(stage!=fault_stage)return 0;
    fault_calls++;return fault_calls==fault_nth?fault_errno:0;
}
int ps5_bt_listener_test_close(int fd) {
    close_calls++;int result=close(fd);
    if(close_interrupt){close_interrupt--;errno=EINTR;return -1;}
    return result;
}
ssize_t ps5_bt_listener_test_recv(int fd,void *output,size_t size,int flags) {
    read_calls++;if(dirty_recv)return (ssize_t)(size+1);
    if(short_io && size>short_io)size=short_io;
    return recv(fd,output,size,flags);
}
ssize_t ps5_bt_listener_test_send(int fd,const void *data,size_t size,int flags) {
    write_calls++;if(dirty_send)return (ssize_t)(size+1);
    if(short_io && size>short_io)size=short_io;
    return send(fd,data,size,flags);
}
static void reset(void) {fault_stage=fault_calls=close_calls=close_interrupt=short_io=dirty_recv=dirty_send=read_calls=write_calls=0;fault_nth=1;fault_errno=EIO;}
union storage {max_align_t align;uint8_t bytes[128];};
struct fixture {union storage memory;struct ps5_bt_listener_diagnostic r;uint16_t port;uint64_t client;int peer;};
static void opened(struct fixture *f) {
    reset();memset(f,0,sizeof *f);f->peer=-1;
    const uint8_t address[4]={127,0,0,1};
    assert(ps5_bt_listener_bind(f->memory.bytes,sizeof f->memory.bytes,address,0,&f->port,&f->r)==0);
    assert(f->port && !f->r.raw_errno && !f->r.flags);
    struct listener *s=(struct listener *)f->memory.bytes;
    assert(fcntl(s->listener_fd,F_GETFL,0)&O_NONBLOCK);
    assert(fcntl(s->listener_fd,F_GETFD,0)&FD_CLOEXEC);
}
static void connected(struct fixture *f) {
    f->peer=socket(AF_INET,SOCK_STREAM,IPPROTO_TCP);assert(f->peer>=0);
    struct sockaddr_in address;memset(&address,0,sizeof address);
#if defined(__APPLE__) || defined(__FreeBSD__)
    address.sin_len=sizeof address;
#endif
    address.sin_family=AF_INET;address.sin_port=htons(f->port);const uint8_t loopback[4]={127,0,0,1};memcpy(&address.sin_addr.s_addr,loopback,4);
    assert(connect(f->peer,(struct sockaddr *)&address,sizeof address)==0);
}
static int accepted(struct fixture *f) {
    for(unsigned n=0;n<100000;n++) {
        int result=ps5_bt_listener_accept(f->memory.bytes,&f->client,&f->r);
        if(result!=PS5_BT_LISTENER_WOULD_BLOCK)return result;
    }
    abort();
}
static int read_ready(struct fixture *f,uint8_t *bytes,uint32_t capacity,uint32_t *count) {
    for(unsigned n=0;n<100000;n++) {
        int rc=ps5_bt_listener_read(f->memory.bytes,f->client,bytes,capacity,count,&f->r);
        if(rc!=PS5_BT_LISTENER_WOULD_BLOCK)return rc;
    }
    abort();
}
static void finished(struct fixture *f) {
    reset();struct listener *s=(struct listener *)f->memory.bytes;
    if(s->magic==MAGIC)assert(ps5_bt_listener_close(f->memory.bytes,&f->r)==0);
    if(f->peer>=0)assert(close(f->peer)==0);
}
static void test_bind_failures_and_flag_setup(void) {
    for(unsigned stage=PS5_BT_LISTENER_SOCKET;stage<=PS5_BT_LISTENER_NAME;stage++) {
        for(unsigned nth=1;nth<=(stage==PS5_BT_LISTENER_NONBLOCK || stage==PS5_BT_LISTENER_CLOEXEC?2u:1u);nth++) {
            struct fixture f;memset(&f,0,sizeof f);reset();fault_stage=stage;fault_nth=nth;
            const uint8_t address[4]={127,0,0,1};f.port=123;
            assert(ps5_bt_listener_bind(f.memory.bytes,sizeof f.memory.bytes,address,0,&f.port,&f.r)==PS5_BT_LISTENER_IO);
            assert(f.port==0 && f.r.stage==stage && f.r.raw_errno==EIO);
            assert(close_calls==(stage==PS5_BT_LISTENER_SOCKET?0u:1u));
            assert(ps5_bt_listener_close(f.memory.bytes,&f.r)==PS5_BT_LISTENER_INVALID);
        }
    }
    struct fixture f;memset(&f,0,sizeof f);reset();fault_stage=PS5_BT_LISTENER_BIND;close_interrupt=1;
    const uint8_t address[4]={127,0,0,1};assert(ps5_bt_listener_bind(f.memory.bytes,sizeof f.memory.bytes,address,0,&f.port,&f.r)==PS5_BT_LISTENER_IO);
    assert(f.r.raw_errno==EIO && f.r.cleanup_errno==EINTR && (f.r.flags&PS5_BT_LISTENER_CLEANUP_UNKNOWN) && close_calls==1);
}
static void test_accept_wouldblock_interrupt_configure_and_single_client(void) {
    struct fixture f;opened(&f);f.client=99;
    assert(ps5_bt_listener_accept(f.memory.bytes,&f.client,&f.r)==PS5_BT_LISTENER_WOULD_BLOCK && f.client==0);
    fault_stage=PS5_BT_LISTENER_ACCEPT;fault_errno=EINTR;
    assert(ps5_bt_listener_accept(f.memory.bytes,&f.client,&f.r)==PS5_BT_LISTENER_IO && f.r.raw_errno==EINTR && f.client==0 && fault_calls==1);
    reset();connected(&f);fault_stage=PS5_BT_LISTENER_NONBLOCK;
    assert(accepted(&f)==PS5_BT_LISTENER_IO && f.client==0 && close_calls==1);
    assert(close(f.peer)==0);f.peer=-1;reset();connected(&f);assert(accepted(&f)==0);
    struct listener *s=(struct listener *)f.memory.bytes;assert(fcntl(s->client_fd,F_GETFL,0)&O_NONBLOCK);assert(fcntl(s->client_fd,F_GETFD,0)&FD_CLOEXEC);
    uint64_t other=99;assert(ps5_bt_listener_accept(f.memory.bytes,&other,&f.r)==PS5_BT_LISTENER_BUSY && other==0);
    finished(&f);
}
static void test_short_io_wouldblock_dirty_counts_and_interrupts(void) {
    struct fixture f;opened(&f);connected(&f);assert(accepted(&f)==0);
    uint8_t bytes[16];uint32_t count=99;
    assert(ps5_bt_listener_read(f.memory.bytes,f.client,bytes,sizeof bytes,&count,&f.r)==PS5_BT_LISTENER_WOULD_BLOCK && count==0);
    reset();fault_stage=PS5_BT_LISTENER_READ;fault_errno=EINTR;
    assert(ps5_bt_listener_read(f.memory.bytes,f.client,bytes,sizeof bytes,&count,&f.r)==PS5_BT_LISTENER_IO && count==0 && f.r.raw_errno==EINTR && fault_calls==1 && read_calls==0);
    reset();assert(send(f.peer,"abcd",4,0)==4);short_io=2;
    for(unsigned n=0;n<2;n++){assert(read_ready(&f,bytes,sizeof bytes,&count)==0 && count==2);assert(!memcmp(bytes,n?"cd":"ab",2));}
    assert(ps5_bt_listener_write(f.memory.bytes,f.client,(const uint8_t *)"abcd",4,&count,&f.r)==0 && count==2);
    assert(recv(f.peer,bytes,2,0)==2 && !memcmp(bytes,"ab",2));
    reset();fault_stage=PS5_BT_LISTENER_WRITE;fault_errno=EAGAIN;
    assert(ps5_bt_listener_write(f.memory.bytes,f.client,(const uint8_t *)"ab",2,&count,&f.r)==PS5_BT_LISTENER_WOULD_BLOCK && count==0 && write_calls==0);
    reset();fault_stage=PS5_BT_LISTENER_WRITE;fault_errno=EINTR;
    assert(ps5_bt_listener_write(f.memory.bytes,f.client,(const uint8_t *)"ab",2,&count,&f.r)==PS5_BT_LISTENER_IO && count==0 && fault_calls==1 && f.r.raw_errno==EINTR);
    reset();dirty_recv=1;assert(ps5_bt_listener_read(f.memory.bytes,f.client,bytes,sizeof bytes,&count,&f.r)==PS5_BT_LISTENER_INVALID && count==0);
    dirty_send=1;assert(ps5_bt_listener_write(f.memory.bytes,f.client,bytes,sizeof bytes,&count,&f.r)==PS5_BT_LISTENER_INVALID && count==0);
    finished(&f);
}
static void test_stale_token_exhaustion_and_close_once(void) {
    struct fixture f;opened(&f);connected(&f);assert(accepted(&f)==0);uint64_t old=f.client;
    reset();assert(ps5_bt_listener_client_close(f.memory.bytes,old,&f.r)==0 && close_calls==1);
    assert(ps5_bt_listener_client_close(f.memory.bytes,old,&f.r)==PS5_BT_LISTENER_INVALID && close_calls==1);
    assert(close(f.peer)==0);f.peer=-1;connected(&f);assert(accepted(&f)==0 && f.client!=old);
    uint8_t bytes[2];uint32_t count;assert(ps5_bt_listener_read(f.memory.bytes,old,bytes,sizeof bytes,&count,&f.r)==PS5_BT_LISTENER_INVALID && count==0);
    reset();close_interrupt=1;assert(ps5_bt_listener_client_close(f.memory.bytes,f.client,&f.r)==PS5_BT_LISTENER_IO && f.r.raw_errno==EINTR && f.r.cleanup_errno==EINTR && close_calls==1);
    assert(ps5_bt_listener_accept(f.memory.bytes,&f.client,&f.r)==PS5_BT_LISTENER_POISONED);
    assert(ps5_bt_listener_close(f.memory.bytes,&f.r)==0 && close_calls==2);
    assert(ps5_bt_listener_close(f.memory.bytes,&f.r)==PS5_BT_LISTENER_INVALID && close_calls==2);finished(&f);
    opened(&f);((struct listener *)f.memory.bytes)->serial=UINT64_MAX;
    assert(ps5_bt_listener_accept(f.memory.bytes,&f.client,&f.r)==PS5_BT_LISTENER_LIMIT && f.client==0);finished(&f);
}
static void test_descriptor_zero_is_owned(void) {
    int saved=dup(0);close(0);struct fixture f;opened(&f);assert(((struct listener *)f.memory.bytes)->listener_fd==0);
    reset();assert(ps5_bt_listener_close(f.memory.bytes,&f.r)==0 && close_calls==1);
    if(saved>=0){assert(dup2(saved,0)==0);assert(close(saved)==0);}
}
static void test_real_sigpipe_suppression_without_global_handler(void) {
    pid_t child=fork();assert(child>=0);
    if(!child){
        signal(SIGPIPE,SIG_DFL);struct fixture f;opened(&f);int pair[2];assert(socketpair(AF_UNIX,SOCK_STREAM,0,pair)==0);
        struct listener *s=(struct listener *)f.memory.bytes;s->client_fd=pair[0];s->client=1;
        assert(configure(pair[0],&f.r)==0);assert(close(pair[1])==0);uint32_t written=99;
        assert(ps5_bt_listener_write(f.memory.bytes,1,(const uint8_t *)"x",1,&written,&f.r)==PS5_BT_LISTENER_IO);
        assert(f.r.raw_errno==EPIPE && written==0);finished(&f);_exit(0);
    }
    int status;assert(waitpid(child,&status,0)==child);assert(WIFEXITED(status) && WEXITSTATUS(status)==0);
}
int main(void) {
    test_bind_failures_and_flag_setup();test_accept_wouldblock_interrupt_configure_and_single_client();
    test_short_io_wouldblock_dirty_counts_and_interrupts();test_stale_token_exhaustion_and_close_once();
    test_descriptor_zero_is_owned();test_real_sigpipe_suppression_without_global_handler();
    puts("PASS loopback listener C faults: flag setup, short I/O, EINTR, WouldBlock, stale token, closeonce and real SIGPIPE");return 0;
}
