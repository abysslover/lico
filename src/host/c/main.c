/* Protocol v1 core in C (POSIX threads), for comparing size and resident memory
 * with other implementations of the same contract. The conformance suite decides
 * whether it is a correct host. Record payloads are a deterministic function of
 * the sequence number; the retained window stores them. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdarg.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#define MAX_REQUEST 4096
#define SNAPSHOT_LINES 40
#define RECORD_BYTES 1024
#define MAX_FIELDS 16
#define STR_MAX 130

typedef struct { char *p; size_t n, cap; } Buf;

static void buf_put(Buf *b, const char *s, size_t n)
{
    if (b->n + n + 1 > b->cap) {
        b->cap = (b->n + n + 1) * 2;
        b->p = realloc(b->p, b->cap);
    }
    memcpy(b->p + b->n, s, n);
    b->n += n;
    b->p[b->n] = 0;
}

static void buf_str(Buf *b, const char *s) { buf_put(b, s, strlen(s)); }

static void buf_fmt(Buf *b, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
static void buf_fmt(Buf *b, const char *fmt, ...)
{
    char tmp[256];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(tmp, sizeof tmp, fmt, ap);
    va_end(ap);
    buf_put(b, tmp, (size_t)n);
}

static void buf_json_str(Buf *b, const char *s)
{
    buf_put(b, "\"", 1);
    for (; *s; s++) {
        unsigned char c = (unsigned char)*s;
        if (c == '"') buf_str(b, "\\\"");
        else if (c == '\\') buf_str(b, "\\\\");
        else if (c < 0x20) buf_fmt(b, "\\u%04x", c);
        else buf_put(b, (const char *)&c, 1);
    }
    buf_put(b, "\"", 1);
}

/* ------------------------------------------------------------- request parse */
enum { T_NULL, T_BOOL, T_INT, T_STR };
typedef struct { char key[24]; int type; long long i; char s[STR_MAX]; int chars; } Field;
typedef struct { Field f[MAX_FIELDS]; int n; } Req;

static const char *ws(const char *p) { while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') p++; return p; }

static int utf8_put(char *out, size_t cap, size_t *len, unsigned cp)
{
    char tmp[4]; int n;
    if (cp < 0x80) { tmp[0] = (char)cp; n = 1; }
    else if (cp < 0x800) { tmp[0] = (char)(0xC0 | (cp >> 6)); tmp[1] = (char)(0x80 | (cp & 63)); n = 2; }
    else { tmp[0] = (char)(0xE0 | (cp >> 12)); tmp[1] = (char)(0x80 | ((cp >> 6) & 63)); tmp[2] = (char)(0x80 | (cp & 63)); n = 3; }
    if (*len + (size_t)n >= cap) return 0;
    memcpy(out + *len, tmp, (size_t)n);
    *len += (size_t)n;
    return 1;
}

static const char *parse_string(const char *p, char *out, size_t cap, int *chars, int *toolong)
{
    size_t len = 0;
    *chars = 0;
    *toolong = 0;
    if (*p++ != '"') return NULL;
    for (;;) {
        unsigned char c = (unsigned char)*p;
        if (c == 0) return NULL;
        p++;
        if (c == '"') break;
        unsigned cp;
        if (c == '\\') {
            char e = *p++;
            switch (e) {
            case '"': cp = '"'; break;
            case '\\': cp = '\\'; break;
            case '/': cp = '/'; break;
            case 'n': cp = '\n'; break;
            case 'r': cp = '\r'; break;
            case 't': cp = '\t'; break;
            case 'b': cp = 8; break;
            case 'f': cp = 12; break;
            case 'u': {
                char hex[5] = {0};
                for (int k = 0; k < 4; k++) { if (!p[k]) return NULL; hex[k] = p[k]; }
                char *end;
                cp = (unsigned)strtoul(hex, &end, 16);
                if (*end || (cp >= 0xD800 && cp <= 0xDFFF)) return NULL;
                p += 4;
                break;
            }
            default: return NULL;
            }
            (*chars)++;
            if (!utf8_put(out, cap, &len, cp)) *toolong = 1;
        } else if (c < 0x20) {
            return NULL;
        } else {
            if ((c & 0xC0) != 0x80) (*chars)++;
            if (len + 1 < cap) out[len++] = (char)c; else *toolong = 1;
        }
    }
    out[len] = 0;
    return p;
}

static int parse_req(const char *text, Req *r)
{
    const char *p = ws(text);
    r->n = 0;
    if (*p++ != '{') return 0;
    p = ws(p);
    if (*p == '}') { p++; }
    else for (;;) {
        if (r->n == MAX_FIELDS) return 0;
        Field *f = &r->f[r->n];
        char key[STR_MAX]; int chars, toolong;
        p = ws(p);
        p = parse_string(p, key, sizeof key, &chars, &toolong);
        if (!p || toolong || strlen(key) >= sizeof f->key) return 0;
        strcpy(f->key, key);
        p = ws(p);
        if (*p++ != ':') return 0;
        p = ws(p);
        if (*p == '"') {
            f->type = T_STR;
            p = parse_string(p, f->s, sizeof f->s, &f->chars, &toolong);
            if (!p) return 0;
            if (toolong) f->chars = 100000;
        } else if (!strncmp(p, "true", 4)) { f->type = T_BOOL; f->i = 1; p += 4; }
        else if (!strncmp(p, "false", 5)) { f->type = T_BOOL; f->i = 0; p += 5; }
        else if (!strncmp(p, "null", 4)) { f->type = T_NULL; p += 4; }
        else if (*p == '-' || (*p >= '0' && *p <= '9')) {
            char *end;
            errno = 0;
            f->i = strtoll(p, &end, 10);
            if (errno || end == p || *end == '.' || *end == 'e' || *end == 'E') return 0;
            f->type = T_INT;
            p = end;
        } else return 0;
        r->n++;
        p = ws(p);
        if (*p == ',') { p++; continue; }
        if (*p == '}') { p++; break; }
        return 0;
    }
    return *ws(p) == 0;
}

static Field *field(Req *r, const char *key)
{
    for (int k = 0; k < r->n; k++) if (!strcmp(r->f[k].key, key)) return &r->f[k];
    return NULL;
}

/* ---------------------------------------------------------------- host state */
typedef struct Node { char *line; struct Node *next; } Node;
typedef struct Session Session;
typedef struct Sub {
    pthread_mutex_t mu;
    pthread_cond_t wake;
    Node *head, *tail;
    size_t count, cap;
    int closed;
    unsigned char *over;
} Sub;

typedef struct { int64_t seq; unsigned char data[RECORD_BYTES]; } Rec;
struct Session {
    char id[20];
    int producing;
    Rec *ring;
    size_t retain, start, len;
    int64_t next_seq, bytes_total;
    Sub **subs;
    size_t nsubs;
};

static pthread_mutex_t host_mu = PTHREAD_MUTEX_INITIALIZER;
static Session *sessions;
static size_t nsessions, retain_n;

static void payload(int64_t seq, unsigned char *out)
{
    uint64_t state = (uint64_t)seq * 0x9E3779B97F4A7C15ULL + 0xD1B54A32D192ED03ULL;
    for (size_t at = 0; at < RECORD_BYTES; at += 8) {
        state += 0x9E3779B97F4A7C15ULL;
        uint64_t z = state;
        z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
        z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
        z ^= z >> 31;
        memcpy(out + at, &z, 8);
    }
}

static void hex_put(Buf *b, const unsigned char *d, size_t n)
{
    static const char digits[] = "0123456789abcdef";
    for (size_t k = 0; k < n; k++) { char c[2] = {digits[d[k] >> 4], digits[d[k] & 15]}; buf_put(b, c, 2); }
}

static void new_id(char *out)
{
    unsigned char r[8];
    FILE *f = fopen("/dev/urandom", "rb");
    if (!f || fread(r, 1, 8, f) != 8) exit(1);
    fclose(f);
    out[0] = 's';
    for (int k = 0; k < 8; k++) sprintf(out + 1 + 2 * k, "%02x", r[k]);
}

static Rec *ring_at(Session *s, size_t i) { return &s->ring[(s->start + i) % s->retain]; }
static int64_t oldest(Session *s) { return s->len ? ring_at(s, 0)->seq : s->next_seq; }

static char *rec_line(Session *s, Rec *r)
{
    Buf b = {0};
    buf_fmt(&b, "{\"v\":1,\"ev\":\"rec\",\"session\":\"%s\",\"generation\":1,\"seq\":%lld,\"data\":\"", s->id, (long long)r->seq);
    hex_put(&b, r->data, RECORD_BYTES);
    buf_str(&b, "\"}\n");
    return b.p;
}

static void push(Sub *sub, char *line)
{
    Node *n = malloc(sizeof *n);
    n->line = line;
    n->next = NULL;
    pthread_mutex_lock(&sub->mu);
    if (sub->tail) sub->tail->next = n; else sub->head = n;
    sub->tail = n;
    sub->count++;
    pthread_cond_signal(&sub->wake);
    pthread_mutex_unlock(&sub->mu);
}

static int sess_index(Session *s) { return (int)(s - sessions); }

/* queue a record for a subscriber; returns 1 when the subscriber must be dropped */
static int offer(Sub *sub, Session *s, const char *line)
{
    int idx = sess_index(s);
    if (sub->over[idx]) return 1;
    if (sub->count >= sub->cap) {
        sub->over[idx] = 1;
        Buf b = {0};
        buf_fmt(&b, "{\"v\":1,\"ev\":\"overflow\",\"session\":\"%s\"}\n", s->id);
        push(sub, b.p);
        return 1; /* never wait for a slow reader */
    }
    push(sub, strdup(line));
    return 0;
}

static void drop_sub(Session *s, Sub *sub)
{
    for (size_t k = 0; k < s->nsubs; k++)
        if (s->subs[k] == sub) { s->subs[k] = s->subs[--s->nsubs]; return; }
}

static void append(Session *s)
{
    Rec *r;
    if (s->len == s->retain) { r = &s->ring[s->start]; s->start = (s->start + 1) % s->retain; }
    else { r = ring_at(s, s->len); s->len++; }
    r->seq = s->next_seq++;
    payload(r->seq, r->data);
    s->bytes_total += RECORD_BYTES;
    if (s->nsubs) {
        char *line = rec_line(s, r);
        for (size_t k = 0; k < s->nsubs;) {
            if (offer(s->subs[k], s, line)) s->subs[k] = s->subs[--s->nsubs]; else k++;
        }
        free(line);
    }
}

static void snapshot_body(Buf *b, Session *s)
{
    size_t skip = s->len > SNAPSHOT_LINES ? s->len - SNAPSHOT_LINES : 0;
    buf_fmt(b, "\"session\":\"%s\",\"generation\":1,\"seq\":%lld,\"lines\":[", s->id, (long long)(s->next_seq - 1));
    for (size_t k = skip; k < s->len; k++) {
        Rec *r = ring_at(s, k);
        if (k > skip) buf_str(b, ",");
        buf_fmt(b, "[%lld,\"%08lld ", (long long)r->seq, (long long)r->seq);
        hex_put(b, r->data, 44);
        buf_str(b, "\"]");
    }
    buf_str(b, "]");
}

/* ------------------------------------------------------------------ handler */
static void reply_head(Buf *b, Field *id, int ok)
{
    buf_fmt(b, "{\"v\":1,\"ok\":%d", ok);
    if (id && id->type == T_STR) { buf_str(b, ",\"id\":"); buf_json_str(b, id->s); }
}

static void fail(Sub *sub, const char *code, Field *id)
{
    Buf b = {0};
    reply_head(&b, id && id->type == T_STR && id->chars <= 64 ? id : NULL, 0);
    buf_fmt(&b, ",\"error\":\"%s\"}\n", code);
    push(sub, b.p);
}

static int valid_request(const char *op, Req *r)
{
    int has_session = strcmp(op, "hello") && strcmp(op, "status");
    int has_after = !strcmp(op, "subscribe");
    int has_generation = !strcmp(op, "snapshot") || has_after;
    for (int k = 0; k < r->n; k++) {
        Field *f = &r->f[k];
        int ok;
        if (!strcmp(f->key, "op")) ok = 1;
        else if (!strcmp(f->key, "v")) ok = f->type == T_INT && f->i == 1;
        else if (!strcmp(f->key, "id")) ok = f->type == T_STR && f->chars <= 64;
        else if (!strcmp(f->key, "session") && has_session) ok = f->type == T_STR && f->chars <= 64;
        else if (!strcmp(f->key, "after") && has_after) ok = f->type == T_INT && f->i >= -1;
        else if (!strcmp(f->key, "generation") && has_generation) ok = f->type == T_INT && f->i >= 1;
        else if (!strcmp(f->key, "snapshot") && has_after) ok = f->type == T_BOOL;
        else ok = 0;
        if (!ok) return 0;
    }
    if (has_session && !field(r, "session")) return 0;
    if (has_after && !field(r, "after")) return 0;
    return 1;
}

static void handle(Sub *sub, const char *text)
{
    static const char *ops[] = {"hello", "status", "snapshot", "subscribe", "unsubscribe"};
    Req req;
    if (!parse_req(text, &req)) { fail(sub, "bad_request", NULL); return; }
    Field *id = field(&req, "id");
    Field *v = field(&req, "v");
    if (v && v->type == T_INT && v->i != 1) { fail(sub, "version_unsupported", id); return; }
    Field *opf = field(&req, "op");
    if (!opf || opf->type != T_STR) { fail(sub, "bad_request", id); return; }
    int known = 0;
    for (size_t k = 0; k < 5; k++) if (!strcmp(opf->s, ops[k])) known = 1;
    if (!known) { fail(sub, "unknown_op", id); return; }
    if (!valid_request(opf->s, &req)) { fail(sub, "bad_request", id); return; }
    const char *op = opf->s;
    Buf b = {0};
    pthread_mutex_lock(&host_mu);
    if (!strcmp(op, "hello")) {
        reply_head(&b, id, 1);
        buf_fmt(&b, ",\"protocol\":1,\"host\":\"lico-core\",\"max_request\":%d,\"retain\":%zu,"
                    "\"ops\":[\"hello\",\"status\",\"snapshot\",\"subscribe\",\"unsubscribe\"]}\n", MAX_REQUEST, retain_n);
        push(sub, b.p);
        pthread_mutex_unlock(&host_mu);
        return;
    }
    if (!strcmp(op, "status")) {
        reply_head(&b, id, 1);
        buf_fmt(&b, ",\"pid\":%d,\"sessions\":[", (int)getpid());
        for (size_t k = 0; k < nsessions; k++) {
            Session *s = &sessions[k];
            if (k) buf_str(&b, ",");
            buf_fmt(&b, "{\"session\":\"%s\",\"generation\":1,\"seq\":%lld,\"producing\":%s,\"subscribers\":%zu,\"bytes_total\":%lld}",
                    s->id, (long long)(s->next_seq - 1), s->producing ? "true" : "false", s->nsubs, (long long)s->bytes_total);
        }
        buf_str(&b, "]}\n");
        push(sub, b.p);
        pthread_mutex_unlock(&host_mu);
        return;
    }
    Session *s = NULL;
    Field *want = field(&req, "session");
    for (size_t k = 0; k < nsessions; k++) if (!strcmp(sessions[k].id, want->s)) s = &sessions[k];
    if (!s) { free(b.p); pthread_mutex_unlock(&host_mu); fail(sub, "unknown_session", id); return; }
    Field *gen = field(&req, "generation");
    if (gen && gen->i != 1) { free(b.p); pthread_mutex_unlock(&host_mu); fail(sub, "stale_generation", id); return; }
    if (!strcmp(op, "snapshot")) {
        reply_head(&b, id, 1);
        buf_str(&b, ",");
        snapshot_body(&b, s);
        buf_str(&b, "}\n");
        push(sub, b.p);
    } else if (!strcmp(op, "unsubscribe")) {
        drop_sub(s, sub);
        sub->over[sess_index(s)] = 0;
        reply_head(&b, id, 1);
        buf_fmt(&b, ",\"session\":\"%s\",\"seq\":%lld}\n", s->id, (long long)(s->next_seq - 1));
        push(sub, b.p);
    } else {
        long long after = field(&req, "after")->i;
        Field *snapf = field(&req, "snapshot");
        int want_snapshot = snapf && snapf->i;
        int64_t old = oldest(s), last = s->next_seq - 1, replay_after = after;
        reply_head(&b, id, 1);
        buf_fmt(&b, ",\"session\":\"%s\",\"generation\":1,", s->id);
        if (want_snapshot) {
            int valid = old <= after + 1 && after <= last;
            buf_fmt(&b, "\"gap\":%s,\"from\":%lld,\"missed\":", valid ? "false" : "true", (long long)(last + 1));
            if (valid) buf_fmt(&b, "%lld", (long long)(last - after)); else buf_str(&b, "null");
            buf_str(&b, ",\"snapshot\":{");
            snapshot_body(&b, s);
            buf_str(&b, "}");
            replay_after = last;
        } else if (after + 1 < old || after > last) {
            buf_str(&b, "\"gap\":true,\"snapshot\":{");
            snapshot_body(&b, s);
            buf_fmt(&b, "},\"from\":%lld", (long long)(last + 1));
            replay_after = last;
        } else {
            buf_fmt(&b, "\"gap\":false,\"from\":%lld", after + 1);
        }
        buf_str(&b, "}\n");
        push(sub, b.p);
        sub->over[sess_index(s)] = 0;
        s->subs = realloc(s->subs, (s->nsubs + 1) * sizeof *s->subs);
        s->subs[s->nsubs++] = sub; /* live records follow the replay with no gap */
        for (size_t k = 0; k < s->len; k++) {
            Rec *r = ring_at(s, k);
            if (r->seq <= replay_after) continue;
            char *line = rec_line(s, r);
            int drop = offer(sub, s, line);
            free(line);
            if (drop) { drop_sub(s, sub); break; }
        }
    }
    pthread_mutex_unlock(&host_mu);
}

/* -------------------------------------------------------------- connections */
static int write_all(int fd, const char *p, size_t n)
{
    while (n) { ssize_t w = write(fd, p, n); if (w <= 0) return -1; p += w; n -= (size_t)w; }
    return 0;
}

typedef struct { int fd; Sub *sub; } WriterArg;

static void *writer_main(void *arg)
{
    WriterArg *w = arg;
    Sub *sub = w->sub;
    for (;;) {
        pthread_mutex_lock(&sub->mu);
        while (!sub->head && !sub->closed) pthread_cond_wait(&sub->wake, &sub->mu);
        Node *n = sub->head;
        if (n) { sub->head = n->next; if (!sub->head) sub->tail = NULL; sub->count--; }
        pthread_mutex_unlock(&sub->mu);
        if (!n) return NULL;
        int bad = write_all(w->fd, n->line, strlen(n->line));
        free(n->line);
        free(n);
        if (bad) return NULL;
    }
}

static void *connection_main(void *arg)
{
    int fd = (int)(long)arg;
    struct ucred cred;
    socklen_t len = sizeof cred;
    if (getsockopt(fd, SOL_SOCKET, SO_PEERCRED, &cred, &len) != 0 || cred.uid != getuid()) { close(fd); return NULL; }
    Sub *sub = calloc(1, sizeof *sub);
    pthread_mutex_init(&sub->mu, NULL);
    pthread_cond_init(&sub->wake, NULL);
    sub->cap = retain_n + 1024;
    sub->over = calloc(nsessions ? nsessions : 1, 1);
    WriterArg warg = {fd, sub};
    pthread_t writer;
    pthread_create(&writer, NULL, writer_main, &warg);
    char buf[2 * MAX_REQUEST + 8];
    size_t have = 0;
    int stop = 0;
    while (!stop) {
        ssize_t n = read(fd, buf + have, sizeof buf - 1 - have);
        if (n <= 0) break;
        have += (size_t)n;
        char *nl;
        while ((nl = memchr(buf, '\n', have)) != NULL) {
            size_t linelen = (size_t)(nl - buf) + 1;
            if (linelen > MAX_REQUEST) {
                push(sub, strdup("{\"v\":1,\"ok\":0,\"error\":\"too_large\"}\n"));
                stop = 1;
                break;
            }
            *nl = 0;
            handle(sub, buf);
            memmove(buf, buf + linelen, have - linelen);
            have -= linelen;
        }
        if (!stop && have > MAX_REQUEST) {
            push(sub, strdup("{\"v\":1,\"ok\":0,\"error\":\"too_large\"}\n"));
            break;
        }
    }
    pthread_mutex_lock(&host_mu);
    for (size_t k = 0; k < nsessions; k++) drop_sub(&sessions[k], sub);
    pthread_mutex_unlock(&host_mu);
    struct timespec nap = {0, 20 * 1000 * 1000};
    nanosleep(&nap, NULL); /* let the writer flush a final error line */
    pthread_mutex_lock(&sub->mu);
    sub->closed = 1;
    pthread_cond_signal(&sub->wake);
    pthread_mutex_unlock(&sub->mu);
    pthread_join(writer, NULL);
    for (Node *n = sub->head; n;) { Node *next = n->next; free(n->line); free(n); n = next; }
    free(sub->over);
    free(sub);
    close(fd);
    return NULL;
}

static void *producer_main(void *arg)
{
    Session *s = arg;
    extern double g_rate;
    struct timespec next;
    clock_gettime(CLOCK_MONOTONIC, &next);
    long step = (long)(1e9 / g_rate);
    for (;;) {
        next.tv_nsec += step;
        while (next.tv_nsec >= 1000000000L) { next.tv_nsec -= 1000000000L; next.tv_sec++; }
        pthread_mutex_lock(&host_mu);
        append(s);
        pthread_mutex_unlock(&host_mu);
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &next, NULL);
    }
    return NULL;
}

double g_rate = 100.0;

static const char *arg_of(int argc, char **argv, const char *name, const char *dflt)
{
    for (int k = 1; k + 1 < argc; k++) if (!strcmp(argv[k], name)) return argv[k + 1];
    return dflt;
}

int main(int argc, char **argv)
{
    const char *path = arg_of(argc, argv, "--socket", "");
    if (!*path) { fprintf(stderr, "usage: lico-host-c --socket PATH [--sessions N] [--producing N] [--rate R] [--history N] [--retain N]\n"); return 2; }
    nsessions = (size_t)atol(arg_of(argc, argv, "--sessions", "1"));
    size_t producing = (size_t)atol(arg_of(argc, argv, "--producing", "1"));
    g_rate = atof(arg_of(argc, argv, "--rate", "100"));
    long history = atol(arg_of(argc, argv, "--history", "5000"));
    retain_n = (size_t)atol(arg_of(argc, argv, "--retain", "2048"));
    sessions = calloc(nsessions, sizeof *sessions);
    for (size_t k = 0; k < nsessions; k++) {
        Session *s = &sessions[k];
        new_id(s->id);
        s->producing = k < producing;
        s->retain = retain_n;
        s->ring = calloc(retain_n, sizeof(Rec));
        for (long h = 0; h < history; h++) append(s);
    }
    unlink(path);
    mode_t old = umask(077); /* never group- or world-accessible, even briefly */
    int lsock = socket(AF_UNIX, SOCK_STREAM, 0);
    struct sockaddr_un addr = {.sun_family = AF_UNIX};
    snprintf(addr.sun_path, sizeof addr.sun_path, "%s", path);
    if (bind(lsock, (struct sockaddr *)&addr, sizeof addr) != 0 || listen(lsock, 64) != 0) { perror("bind"); return 1; }
    umask(old);
    for (size_t k = 0; k < nsessions && k < producing; k++) {
        pthread_t t;
        pthread_create(&t, NULL, producer_main, &sessions[k]);
        pthread_detach(t);
    }
    printf("{\"ready\":1,\"pid\":%d,\"sessions\":[", (int)getpid());
    for (size_t k = 0; k < nsessions; k++) printf("%s\"%s\"", k ? "," : "", sessions[k].id);
    printf("]}\n");
    fflush(stdout);
    for (;;) {
        int c = accept(lsock, NULL, NULL);
        if (c < 0) continue;
        pthread_t t;
        pthread_create(&t, NULL, connection_main, (void *)(long)c);
        pthread_detach(t);
    }
}
