/* Candidate B: GTK 3 client for the synthetic host (text widget view). */
#include <gtk/gtk.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#define VISIBLE_LINES 40
#define LINE_HEX 88

static int sock_fd = -1;
static GString *sock_in, *cmd_in;
static char cur_session[40];
static gint64 cursor = -1, dups, gaps, resyncs;
static gboolean subscribed;
static GtkWidget *window, *root, *placeholder, *view, *textview, *mark_session, *mark_seq;
static GtkTextBuffer *tbuf;
static int lines_in_buf;
static char **meta_ids;
static gint64 *meta_cursor;
static int meta_n;
static gboolean print_status;

static void emit_state(const char *why)
{
    printf("{\"ev\":\"state\",\"why\":\"%s\",\"session\":\"%s\",\"view_resident\":%s,"
           "\"line_buffer_lines\":%d,\"cursor\":%lld,\"subscribed\":%s,\"dups\":%lld,\"gaps\":%lld,\"resyncs\":%lld,\"known_sessions\":%d}\n",
           why, cur_session, view ? "true" : "false", view ? lines_in_buf : 0,
           (long long)cursor, subscribed ? "true" : "false", (long long)dups, (long long)gaps, (long long)resyncs, meta_n);
    fflush(stdout);
}

static void send_line(const char *line)
{
    if (sock_fd >= 0) {
        (void)!write(sock_fd, line, strlen(line));
        (void)!write(sock_fd, "\n", 1);
    }
}

static gboolean jint(const char *s, const char *key, gint64 *out)
{
    char pat[48];
    snprintf(pat, sizeof pat, "\"%s\":", key);
    const char *p = strstr(s, pat);
    if (!p) return FALSE;
    *out = strtoll(p + strlen(pat), NULL, 10);
    return TRUE;
}

static gboolean jstr(const char *s, const char *key, char *out, size_t n)
{
    char pat[48];
    snprintf(pat, sizeof pat, "\"%s\":\"", key);
    const char *p = strstr(s, pat);
    if (!p) return FALSE;
    p += strlen(pat);
    const char *e = strchr(p, '"');
    if (!e) return FALSE;
    size_t len = MIN((size_t)(e - p), n - 1);
    memcpy(out, p, len);
    out[len] = 0;
    return TRUE;
}

static gboolean draw_mark(GtkWidget *w, cairo_t *cr, gpointer data)
{
    guint32 rgb = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(w), "rgb"));
    cairo_set_source_rgb(cr, ((rgb >> 16) & 255) / 255.0, ((rgb >> 8) & 255) / 255.0, (rgb & 255) / 255.0);
    cairo_paint(cr);
    return TRUE;
}

static void set_mark(GtkWidget *w, guint32 rgb)
{
    g_object_set_data(G_OBJECT(w), "rgb", GUINT_TO_POINTER(rgb));
    gtk_widget_queue_draw(w);
}

static void append_line(gint64 seq, const char *hex)
{
    char line[160];
    GtkTextIter end, a, b;
    snprintf(line, sizeof line, "%08lld %.*s\n", (long long)seq, LINE_HEX, hex);
    gtk_text_buffer_get_end_iter(tbuf, &end);
    gtk_text_buffer_insert(tbuf, &end, line, -1);
    lines_in_buf++;
    if (lines_in_buf > VISIBLE_LINES) {
        gtk_text_buffer_get_start_iter(tbuf, &a);
        gtk_text_buffer_get_iter_at_line(tbuf, &b, 1);
        gtk_text_buffer_delete(tbuf, &a, &b);
        lines_in_buf--;
    }
    gtk_text_buffer_get_end_iter(tbuf, &end);
    GtkTextMark *m = gtk_text_buffer_create_mark(tbuf, NULL, &end, FALSE);
    gtk_text_view_scroll_mark_onscreen(GTK_TEXT_VIEW(textview), m);
    gtk_text_buffer_delete_mark(tbuf, m);
    cursor = seq;
    set_mark(mark_seq, (guint32)(seq & 0xffffff));
}

static void build_view(void)
{
    GtkWidget *top = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    GtkWidget *title = gtk_label_new("\xed\x95\x9c\xea\xb8\x80 Lico session view");
    GtkWidget *scroll = gtk_scrolled_window_new(NULL, NULL);
    view = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    mark_session = gtk_drawing_area_new();
    mark_seq = gtk_drawing_area_new();
    gtk_widget_set_size_request(mark_session, 16, 16);
    gtk_widget_set_size_request(mark_seq, 16, 16);
    g_signal_connect(mark_session, "draw", G_CALLBACK(draw_mark), NULL);
    g_signal_connect(mark_seq, "draw", G_CALLBACK(draw_mark), NULL);
    textview = gtk_text_view_new();
    gtk_text_view_set_editable(GTK_TEXT_VIEW(textview), FALSE);
    gtk_text_view_set_monospace(GTK_TEXT_VIEW(textview), TRUE);
    tbuf = gtk_text_view_get_buffer(GTK_TEXT_VIEW(textview));
    lines_in_buf = 0;
    gtk_container_add(GTK_CONTAINER(scroll), textview);
    gtk_box_pack_start(GTK_BOX(top), mark_session, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(top), mark_seq, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(top), title, TRUE, TRUE, 4);
    gtk_box_pack_start(GTK_BOX(view), top, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(view), scroll, TRUE, TRUE, 0);
    gtk_box_pack_start(GTK_BOX(root), view, TRUE, TRUE, 0);
    gtk_widget_show_all(view);
}

static int meta_find(const char *id)
{
    for (int i = 0; i < meta_n; i++)
        if (!strcmp(meta_ids[i], id)) return i;
    return -1;
}

static void on_status_reply(const char *line)
{
    const char *p = line;
    for (int i = 0; i < meta_n; i++) g_free(meta_ids[i]);
    g_free(meta_ids);
    g_free(meta_cursor);
    meta_n = 0;
    int cap = 0;
    meta_ids = NULL;
    meta_cursor = NULL;
    while ((p = strstr(p, "\"session\":\"")) != NULL) {
        p += 11;
        const char *e = strchr(p, '"');
        if (!e) break;
        if (meta_n == cap) {
            cap = cap ? cap * 2 : 16;
            meta_ids = g_realloc(meta_ids, cap * sizeof *meta_ids);
            meta_cursor = g_realloc(meta_cursor, cap * sizeof *meta_cursor);
        }
        meta_ids[meta_n] = g_strndup(p, e - p);
        meta_cursor[meta_n++] = -1;
        p = e;
    }
}

static void hide_view(void)
{
    int known = cur_session[0] ? meta_find(cur_session) : -1;
    if (known >= 0) meta_cursor[known] = cursor; /* navigation metadata survives; the view does not */
    if (subscribed && cur_session[0]) {
        char msg[128];
        snprintf(msg, sizeof msg, "{\"op\":\"unsubscribe\",\"session\":\"%s\"}", cur_session);
        send_line(msg);
    }
    subscribed = FALSE;
    if (view) {
        gtk_widget_destroy(view); /* releases the view tree, text buffer and marker widgets */
        view = textview = mark_session = mark_seq = NULL;
        tbuf = NULL;
        lines_in_buf = 0;
    }
}

static void show_session(const char *id, gint64 after, gboolean has_after)
{
    char msg[160];
    hide_view();
    snprintf(cur_session, sizeof cur_session, "%s", id);
    int known = meta_find(id);
    /* an explicit cursor replays after it; otherwise ask for the latest screen */
    if (has_after) cursor = after;
    else if (known >= 0) cursor = meta_cursor[known];
    else cursor = -1;
    build_view();
    char head[7] = {0};
    memcpy(head, id + 1, 6);
    guint32 rgb = (guint32)strtoul(head, NULL, 16) & 0xffffff; /* first six hex digits */
    set_mark(mark_session, rgb);
    snprintf(msg, sizeof msg, "{\"op\":\"subscribe\",\"session\":\"%s\",\"after\":%lld%s}", id,
             (long long)cursor, has_after ? "" : ",\"snapshot\":true");
    send_line(msg);
}

static void on_snapshot_lines(const char *reply)
{
    const char *p = strstr(reply, "\"lines\":[");
    if (!p) return;
    p += 9;
    while ((p = strchr(p, '[')) != NULL) {
        char *q;
        gint64 seq = strtoll(p + 1, &q, 10);
        const char *s = strchr(q, '"');
        if (!s) break;
        s++;
        const char *e = strchr(s, '"');
        if (!e) break;
        char hex[LINE_HEX + 1];
        size_t full = (size_t)(e - s);
        size_t len = full > 9 ? MIN(full - 9, (size_t)LINE_HEX) : 0;
        memcpy(hex, s + 9, len);
        hex[len] = 0;
        append_line(seq, hex);
        p = e;
    }
}

static void handle_sock_line(const char *line)
{
    char ev[24], sess[40];
    gint64 seq;
    if (jstr(line, "ev", ev, sizeof ev) && !strcmp(ev, "rec")) {
        if (!view || !jstr(line, "session", sess, sizeof sess) || strcmp(sess, cur_session) || !jint(line, "seq", &seq))
            return;
        if (seq <= cursor) { dups++; return; }
        if (seq != cursor + 1 && cursor >= 0) gaps++;
        char data[2200];
        if (jstr(line, "data", data, sizeof data)) append_line(seq, data);
        return;
    }
    if (jstr(line, "ev", ev, sizeof ev) && !strcmp(ev, "overflow")) {
        /* the host dropped this stream: fetch a fresh snapshot instead of guessing */
        char again[160];
        subscribed = FALSE;
        resyncs++;
        emit_state("overflow");
        snprintf(again, sizeof again, "{\"op\":\"subscribe\",\"session\":\"%s\",\"after\":%lld,\"snapshot\":true}",
                 cur_session, (long long)cursor);
        send_line(again);
        return;
    }
    if (strstr(line, "\"ok\":1") && strstr(line, "\"gap\":")) {
        subscribed = TRUE;
        if (strstr(line, "\"snapshot\":") && view) {
            gtk_text_buffer_set_text(tbuf, "", -1);
            lines_in_buf = 0;
            on_snapshot_lines(line);
            gint64 snap;
            const char *sp = strstr(line, "\"snapshot\":");
            if (sp && jint(sp, "seq", &snap)) cursor = snap;
        }
        emit_state("subscribed");
    } else if (strstr(line, "\"sessions\":[")) {
        on_status_reply(line);
        if (print_status) {
            printf("%s\n", line);
            fflush(stdout);
        }
        print_status = FALSE;
        emit_state("loaded");
    }
}

static gboolean on_socket(GIOChannel *ch, GIOCondition cond, gpointer data)
{
    char buf[8192];
    ssize_t n = read(sock_fd, buf, sizeof buf);
    if (n <= 0) { gtk_main_quit(); return FALSE; }
    g_string_append_len(sock_in, buf, n);
    char *nl;
    while ((nl = memchr(sock_in->str, '\n', sock_in->len)) != NULL) {
        *nl = 0;
        handle_sock_line(sock_in->str);
        g_string_erase(sock_in, 0, nl - sock_in->str + 1);
    }
    return TRUE;
}

static void handle_command(char *line)
{
    char id[40];
    long long after;
    if (sscanf(line, "show %39s %lld", id, &after) == 2) show_session(id, after, TRUE);
    else if (sscanf(line, "show %39s", id) == 1) show_session(id, -1, FALSE);
    else if (!strncmp(line, "hide", 4)) { hide_view(); emit_state("hidden"); }
    else if (!strncmp(line, "status", 6)) emit_state("status");
    else if (!strncmp(line, "hoststatus", 10)) { print_status = TRUE; send_line("{\"op\":\"status\"}"); }
    else if (!strncmp(line, "load", 4)) send_line("{\"op\":\"status\"}");
    else if (!strncmp(line, "quit", 4)) gtk_main_quit();
}

static gboolean on_stdin(GIOChannel *ch, GIOCondition cond, gpointer data)
{
    char buf[512];
    ssize_t n = read(0, buf, sizeof buf);
    if (n <= 0) { gtk_main_quit(); return FALSE; }
    g_string_append_len(cmd_in, buf, n);
    char *nl;
    while ((nl = memchr(cmd_in->str, '\n', cmd_in->len)) != NULL) {
        *nl = 0;
        handle_command(cmd_in->str);
        g_string_erase(cmd_in, 0, nl - cmd_in->str + 1);
    }
    return TRUE;
}

int main(int argc, char **argv)
{
    const char *path = NULL;
    for (int i = 1; i + 1 < argc; i++)
        if (!strcmp(argv[i], "--socket")) path = argv[i + 1];
    if (!path) { fprintf(stderr, "usage: candidate-b --socket PATH\n"); return 2; }
    gtk_init(&argc, &argv);
    struct sockaddr_un addr = { .sun_family = AF_UNIX };
    snprintf(addr.sun_path, sizeof addr.sun_path, "%s", path);
    sock_fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (connect(sock_fd, (struct sockaddr *)&addr, sizeof addr) != 0) { perror("connect"); return 1; }
    sock_in = g_string_new(NULL);
    cmd_in = g_string_new(NULL);
    window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_default_size(GTK_WINDOW(window), 1200, 800);
    gtk_window_set_title(GTK_WINDOW(window), "Lico candidate B");
    g_signal_connect(window, "destroy", G_CALLBACK(gtk_main_quit), NULL);
    root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_container_add(GTK_CONTAINER(window), root);
    placeholder = gtk_label_new("no session view");
    gtk_box_pack_end(GTK_BOX(root), placeholder, FALSE, FALSE, 0);
    gtk_widget_show_all(window);
    g_io_add_watch(g_io_channel_unix_new(sock_fd), G_IO_IN | G_IO_HUP, on_socket, NULL);
    g_io_add_watch(g_io_channel_unix_new(0), G_IO_IN | G_IO_HUP, on_stdin, NULL);
    printf("{\"ev\":\"ready\",\"pid\":%d}\n", (int)getpid());
    fflush(stdout);
    gtk_main();
    return 0;
}
