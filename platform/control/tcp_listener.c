/* SPDX-License-Identifier: GPL-3.0-or-later
 * Original POSIX candidate: system headers determine all native layouts/flags.
 * Pinned source review/provenance: docs/CONTROL-LISTENER.md. No PS5 activation.
 */
#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE
#define _DARWIN_C_SOURCE
#include "tcp_listener.h"
#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <unistd.h>

#define MAGIC UINT32_C(0x50354c4e)
struct listener { uint32_t magic; int listener_fd,client_fd,poisoned; uint64_t serial,client; };
#ifdef PS5_BT_LISTENER_TEST
extern int ps5_bt_listener_test_fault(uint32_t);
extern int ps5_bt_listener_test_close(int);
extern ssize_t ps5_bt_listener_test_recv(int,void *,size_t,int);
extern ssize_t ps5_bt_listener_test_send(int,const void *,size_t,int);
#define CLOSE ps5_bt_listener_test_close
#define RECV ps5_bt_listener_test_recv
#define SEND ps5_bt_listener_test_send
static int fault(uint32_t stage) { int error=ps5_bt_listener_test_fault(stage); if(error) { errno=error;return 1; } return 0; }
#define FAULT(stage) fault(stage)
#else
#define CLOSE close
#define RECV recv
#define SEND send
#define FAULT(stage) ((void)(stage),0)
#endif
static void report(struct ps5_bt_listener_diagnostic *r) { memset(r,0,sizeof *r);r->abi_version=PS5_BT_LISTENER_ABI_VERSION; }
static int result(struct ps5_bt_listener_diagnostic *r,int status,unsigned stage,int error) { r->status=status;r->stage=stage;r->raw_errno=error;return status; }
static int io(struct ps5_bt_listener_diagnostic *r,unsigned stage) { int error=errno;return result(r,PS5_BT_LISTENER_IO,stage,error); }
static int close_once(int *owned,struct ps5_bt_listener_diagnostic *r) {
    if(*owned<0)return 0;
    int fd=*owned;*owned=-1;
    if(CLOSE(fd)) { int error=errno;if(!r->cleanup_errno)r->cleanup_errno=error;r->flags|=PS5_BT_LISTENER_CLEANUP_UNKNOWN;return -1; }
    return 0;
}
static int context(struct listener *s,struct ps5_bt_listener_diagnostic *r) {
    if(!s || s->magic!=MAGIC || s->listener_fd<0)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    if(s->poisoned)return result(r,PS5_BT_LISTENER_POISONED,PS5_BT_LISTENER_VALIDATE,0);
    return 0;
}
static int configure(int fd,struct ps5_bt_listener_diagnostic *r) {
    int flags=FAULT(PS5_BT_LISTENER_NONBLOCK)?-1:fcntl(fd,F_GETFL,0);
    if(flags<0 || FAULT(PS5_BT_LISTENER_NONBLOCK) || fcntl(fd,F_SETFL,flags|O_NONBLOCK)<0)return io(r,PS5_BT_LISTENER_NONBLOCK);
    flags=FAULT(PS5_BT_LISTENER_CLOEXEC)?-1:fcntl(fd,F_GETFD,0);
    if(flags<0 || FAULT(PS5_BT_LISTENER_CLOEXEC) || fcntl(fd,F_SETFD,flags|FD_CLOEXEC)<0)return io(r,PS5_BT_LISTENER_CLOEXEC);
#if defined(SO_NOSIGPIPE)
    int enabled=1;
    if(FAULT(PS5_BT_LISTENER_SIGPIPE) || setsockopt(fd,SOL_SOCKET,SO_NOSIGPIPE,&enabled,sizeof enabled))return io(r,PS5_BT_LISTENER_SIGPIPE);
#elif defined(MSG_NOSIGNAL)
    if(FAULT(PS5_BT_LISTENER_SIGPIPE))return io(r,PS5_BT_LISTENER_SIGPIPE);
#else
    return result(r,PS5_BT_LISTENER_UNSUPPORTED,PS5_BT_LISTENER_SIGPIPE,0);
#endif
    return 0;
}
uint32_t ps5_bt_listener_abi_version(void) {return PS5_BT_LISTENER_ABI_VERSION;}
uint32_t ps5_bt_listener_storage_size(void) {return (uint32_t)sizeof(struct listener);}
uint32_t ps5_bt_listener_storage_alignment(void) {return (uint32_t)_Alignof(struct listener);}
int32_t ps5_bt_listener_bind(void *storage,uint32_t capacity,const uint8_t address[4],uint16_t port,uint16_t *bound_port,struct ps5_bt_listener_diagnostic *r) {
    if(!r)return PS5_BT_LISTENER_INVALID;
    report(r);if(bound_port)*bound_port=0;
    static const uint8_t loopback[4]={127,0,0,1};
    if(!storage || capacity<sizeof(struct listener) || (uintptr_t)storage%_Alignof(struct listener) || !address || !bound_port || address[0]==0 || address[0]>=224 || (!port && memcmp(address,loopback,4)))return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    struct listener *s=storage;
    if(s->magic==MAGIC)return result(r,PS5_BT_LISTENER_BUSY,PS5_BT_LISTENER_VALIDATE,0);
    memset(s,0,sizeof *s);s->listener_fd=s->client_fd=-1;
    int status=0;
    s->listener_fd=FAULT(PS5_BT_LISTENER_SOCKET)?-1:socket(AF_INET,SOCK_STREAM,IPPROTO_TCP);
    if(s->listener_fd<0)return io(r,PS5_BT_LISTENER_SOCKET);
    if((status=configure(s->listener_fd,r)))goto failed;
    struct sockaddr_in local;memset(&local,0,sizeof local);
#if defined(__APPLE__) || defined(__FreeBSD__) || defined(PS5_BT_LISTENER_SOCKADDR_HAS_LEN)
    local.sin_len=sizeof local;
#endif
    local.sin_family=AF_INET;local.sin_port=htons(port);memcpy(&local.sin_addr.s_addr,address,4);
    if(FAULT(PS5_BT_LISTENER_BIND) || bind(s->listener_fd,(const struct sockaddr *)&local,sizeof local)) {status=io(r,PS5_BT_LISTENER_BIND);goto failed;}
    if(FAULT(PS5_BT_LISTENER_LISTEN) || listen(s->listener_fd,1)) {status=io(r,PS5_BT_LISTENER_LISTEN);goto failed;}
    socklen_t length=sizeof local;
    if(FAULT(PS5_BT_LISTENER_NAME) || getsockname(s->listener_fd,(struct sockaddr *)&local,&length)) {status=io(r,PS5_BT_LISTENER_NAME);goto failed;}
    if(length!=sizeof local || local.sin_family!=AF_INET || memcmp(&local.sin_addr.s_addr,address,4) || !local.sin_port || (port && ntohs(local.sin_port)!=port)) {status=result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_NAME,0);goto failed;}
    *bound_port=ntohs(local.sin_port);s->magic=MAGIC;return result(r,0,PS5_BT_LISTENER_BIND,0);
failed:
    close_once(&s->listener_fd,r);return status;
}
int32_t ps5_bt_listener_accept(void *storage,uint64_t *client,struct ps5_bt_listener_diagnostic *r) {
    if(!r)return PS5_BT_LISTENER_INVALID;
    report(r);if(client)*client=0;if(!client)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    struct listener *s=storage;int status=context(s,r);if(status)return status;
    if(s->client_fd>=0)return result(r,PS5_BT_LISTENER_BUSY,PS5_BT_LISTENER_ACCEPT,0);
    if(s->serial==UINT64_MAX)return result(r,PS5_BT_LISTENER_LIMIT,PS5_BT_LISTENER_ACCEPT,0);
    s->client_fd=FAULT(PS5_BT_LISTENER_ACCEPT)?-1:accept(s->listener_fd,NULL,NULL);
    if(s->client_fd<0) {int error=errno;return result(r,(error==EAGAIN || error==EWOULDBLOCK)?PS5_BT_LISTENER_WOULD_BLOCK:PS5_BT_LISTENER_IO,PS5_BT_LISTENER_ACCEPT,error);}
    if((status=configure(s->client_fd,r))) {if(close_once(&s->client_fd,r))s->poisoned=1;return status;}
    s->client=++s->serial;*client=s->client;return result(r,0,PS5_BT_LISTENER_ACCEPT,0);
}
static int valid_client(struct listener *s,uint64_t token,struct ps5_bt_listener_diagnostic *r) {
    int status=context(s,r);if(status)return status;
    if(!token || s->client!=token || s->client_fd<0)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    return 0;
}
int32_t ps5_bt_listener_read(void *storage,uint64_t token,uint8_t *output,uint32_t capacity,uint32_t *length,struct ps5_bt_listener_diagnostic *r) {
    if(!r)return PS5_BT_LISTENER_INVALID;report(r);if(length)*length=0;
    if(!output || !length || !capacity || capacity>PS5_BT_LISTENER_IO_MAX)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    struct listener *s=storage;int status=valid_client(s,token,r);if(status)return status;
    ssize_t got=FAULT(PS5_BT_LISTENER_READ)?-1:RECV(s->client_fd,output,capacity,0);
    if(got<0) {int error=errno;return result(r,(error==EAGAIN || error==EWOULDBLOCK)?PS5_BT_LISTENER_WOULD_BLOCK:PS5_BT_LISTENER_IO,PS5_BT_LISTENER_READ,error);}
    if((uint64_t)got>capacity)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_READ,0);
    *length=(uint32_t)got;return result(r,0,PS5_BT_LISTENER_READ,0);
}
int32_t ps5_bt_listener_write(void *storage,uint64_t token,const uint8_t *data,uint32_t length,uint32_t *written,struct ps5_bt_listener_diagnostic *r) {
    if(!r)return PS5_BT_LISTENER_INVALID;report(r);if(written)*written=0;
    if(!data || !written || !length || length>PS5_BT_LISTENER_IO_MAX)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    struct listener *s=storage;int status=valid_client(s,token,r);if(status)return status;
    int flags=0;
#if !defined(SO_NOSIGPIPE) && defined(MSG_NOSIGNAL)
    flags=MSG_NOSIGNAL;
#endif
    ssize_t put=FAULT(PS5_BT_LISTENER_WRITE)?-1:SEND(s->client_fd,data,length,flags);
    if(put<0) {int error=errno;return result(r,(error==EAGAIN || error==EWOULDBLOCK)?PS5_BT_LISTENER_WOULD_BLOCK:PS5_BT_LISTENER_IO,PS5_BT_LISTENER_WRITE,error);}
    if((uint64_t)put>length)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_WRITE,0);
    *written=(uint32_t)put;return result(r,0,PS5_BT_LISTENER_WRITE,0);
}
int32_t ps5_bt_listener_client_close(void *storage,uint64_t token,struct ps5_bt_listener_diagnostic *r) {
    if(!r)return PS5_BT_LISTENER_INVALID;report(r);
    struct listener *s=storage;
    if(!s || s->magic!=MAGIC || !token || s->client!=token || s->client_fd<0)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    s->client=0;
    if(close_once(&s->client_fd,r)) {s->poisoned=1;return result(r,PS5_BT_LISTENER_IO,PS5_BT_LISTENER_CLIENT_CLOSE,r->cleanup_errno);}
    return result(r,0,PS5_BT_LISTENER_CLIENT_CLOSE,0);
}
int32_t ps5_bt_listener_close(void *storage,struct ps5_bt_listener_diagnostic *r) {
    if(!r)return PS5_BT_LISTENER_INVALID;report(r);struct listener *s=storage;
    if(!s || s->magic!=MAGIC)return result(r,PS5_BT_LISTENER_INVALID,PS5_BT_LISTENER_VALIDATE,0);
    s->magic=0;s->client=0;int first=close_once(&s->client_fd,r),second=close_once(&s->listener_fd,r);
    return result(r,(first || second)?PS5_BT_LISTENER_IO:0,PS5_BT_LISTENER_CLOSE,r->cleanup_errno);
}
