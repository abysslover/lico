// Candidate C: Qt 5 Widgets client for the synthetic host (text widget view).
// No signals or slots are declared here, so no moc step is needed.
#include <QApplication>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QInputMethod>
#include <QEventLoop>
#include <QInputMethodEvent>
#include <QLineEdit>
#include <QLabel>
#include <QPaintEvent>
#include <QPainter>
#include <QPlainTextEdit>
#include <QSocketNotifier>
#include <QVBoxLayout>
#include <QWidget>
#include <QCoreApplication>
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <string>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <vector>

static const int VISIBLE_LINES = 40;
static const int LINE_HEX = 88;
static const int LINE_CHARS = 200;

class Mark : public QWidget {
public:
    QColor color{0, 0, 0};
    Mark() { setFixedSize(16, 16); }
    void set(unsigned rgb) { color = QColor((rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255); update(); }
protected:
    void paintEvent(QPaintEvent *) override { QPainter(this).fillRect(rect(), color); }
};

static int sock_fd = -1;
static std::string sock_in, cmd_in, cur_session;
static long long cursor = -1, dups = 0, gaps = 0, resyncs = 0;
static bool subscribed = false, print_status = false;
static std::string view_id;
static long long generation = 1;
static unsigned long sent_count;
static std::map<std::string, std::string> drafts;
static std::map<std::string, std::pair<std::string, std::string>> pending_inputs; /* request id -> (session, text) until answered */
static std::vector<std::string> meta_ids;
static std::vector<long long> meta_cursor;
static void emit_ime(const char *kind, const QString &text)
{
    printf("{\"ev\":\"ime\",\"kind\":\"%s\",\"text\":\"%s\"}\n", kind, text.toUtf8().constData());
    fflush(stdout);
}

static QString last_preedit;       /* what is composing right now */
static bool stale_pending;         /* a composition of a session we left may still be committed by the input method */
static bool stale_empty;
static bool commit_seen;           /* a commit arrived (used to wait for the answer to a reset) */

static qint64 now_ms()
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (qint64)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

class InputLine : public QLineEdit {
protected:
    void inputMethodEvent(QInputMethodEvent *e) override
    {
        last_preedit = e->preeditString();
        if (stale_pending) {
            if (e->preeditString().isEmpty()) stale_empty = true;           /* a commit usually follows an empty preedit */
            else if (stale_empty && e->commitString().isEmpty()) stale_pending = false; /* a new composition began */
        }
        emit_ime("preedit", e->preeditString());
        if (!e->commitString().isEmpty()) {
            commit_seen = true;
            if (stale_pending) {
                emit_ime("ignored", e->commitString()); /* the old composition finishing, not new typing */
                stale_pending = false;
                e->setCommitString(QString());
            } else {
                emit_ime("commit", e->commitString());
            }
        }
        QLineEdit::inputMethodEvent(e);
    }
};

static QWidget *window, *view;
static InputLine *entry;
static QVBoxLayout *root;
static QLabel *placeholder;
static QPlainTextEdit *edit;
static Mark *mark_session, *mark_seq;
static int lines_in_buf;

static void emit_state(const char *why)
{
    printf("{\"ev\":\"state\",\"why\":\"%s\",\"session\":\"%s\",\"view_resident\":%s,\"line_buffer_lines\":%d,"
           "\"cursor\":%lld,\"subscribed\":%s,\"dups\":%lld,\"gaps\":%lld,\"resyncs\":%lld,\"known_sessions\":%d}\n",
           why, cur_session.c_str(), view ? "true" : "false", view ? lines_in_buf : 0, cursor,
           subscribed ? "true" : "false", dups, gaps, resyncs, (int)meta_ids.size());
    fflush(stdout);
}

static void send_line(const std::string &line)
{
    if (sock_fd >= 0) {
        std::string out = line + "\n";
        (void)!write(sock_fd, out.data(), out.size());
    }
}

static bool jint(const char *s, const char *key, long long *out)
{
    std::string pat = std::string("\"") + key + "\":";
    const char *p = strstr(s, pat.c_str());
    if (!p) return false;
    *out = strtoll(p + pat.size(), nullptr, 10);
    return true;
}

static bool jstr(const char *s, const char *key, std::string *out)
{
    std::string pat = std::string("\"") + key + "\":\"";
    const char *p = strstr(s, pat.c_str());
    if (!p) return false;
    p += pat.size();
    const char *e = strchr(p, '"');
    if (!e) return false;
    out->assign(p, e - p);
    return true;
}

static int meta_find(const std::string &id)
{
    for (size_t i = 0; i < meta_ids.size(); i++)
        if (meta_ids[i] == id) return (int)i;
    return -1;
}

static void on_status_reply(const char *line)
{
    meta_ids.clear();
    meta_cursor.clear();
    const char *p = line;
    while ((p = strstr(p, "\"session\":\"")) != nullptr) {
        p += 11;
        const char *e = strchr(p, '"');
        if (!e) break;
        meta_ids.emplace_back(p, e - p);
        meta_cursor.push_back(-1);
        p = e;
    }
}

static std::string json_escape(const std::string &s)
{
    std::string out;
    for (unsigned char c : s) {
        if (c == '"') out += "\\\"";
        else if (c == '\\') out += "\\\\";
        else if (c < 0x20) { char b[8]; snprintf(b, sizeof b, "\\u%04x", c); out += b; }
        else out += (char)c;
    }
    return out;
}

/* send the draft of the visible session as one input with its own request id */
static void send_draft()
{
    if (!entry || cur_session.empty() || entry->text().isEmpty()) return;
    std::string text = entry->text().toUtf8().constData();
    char request_id[64];
    snprintf(request_id, sizeof request_id, "gui-%d-%lu", (int)getpid(), ++sent_count);
    char head[260];
    snprintf(head, sizeof head, "{\"op\":\"input\",\"id\":\"%s\",\"session\":\"%s\",\"generation\":%lld,\"request_id\":\"%s\",\"text\":\"",
             request_id, cur_session.c_str(), generation, request_id);
    pending_inputs[request_id] = std::make_pair(cur_session, text);
    send_line(std::string(head) + json_escape(text) + "\"}");
    emit_ime("sent", QString::fromStdString(text));
    entry->clear();
}

static void append_line(long long seq, const std::string &hex)
{
    if (!view) return;
    char line[160];
    snprintf(line, sizeof line, "%08lld %.*s", seq, LINE_HEX, hex.c_str());
    edit->appendPlainText(QString::fromLatin1(line));
    lines_in_buf = std::min(lines_in_buf + 1, VISIBLE_LINES);
    cursor = seq;
    mark_seq->set((unsigned)(seq & 0xffffff));
}

/* text from an agent is shown, never obeyed: control characters and the bidirectional overrides are dropped */
static QString clean(const QString &text)
{
    QString out;
    for (QChar c : text) {
        ushort u = c.unicode();
        if (u < 0x20 || (u >= 0x7f && u <= 0x9f) || (u >= 0x202a && u <= 0x202e) || (u >= 0x2066 && u <= 0x2069)) continue;
        out.append(c);
        if (out.size() >= LINE_CHARS) break;
    }
    return out;
}

static void append_text(long long seq, const QString &text)
{
    if (!view) return;
    edit->appendPlainText(clean(text));
    lines_in_buf = std::min(lines_in_buf + 1, VISIBLE_LINES);
    cursor = seq;
    mark_seq->set((unsigned)(seq & 0xffffff));
}

static void build_view()
{
    view = new QWidget;
    QVBoxLayout *column = new QVBoxLayout(view);
    column->setContentsMargins(0, 0, 0, 0);
    column->setSpacing(0);
    QHBoxLayout *top = new QHBoxLayout;
    top->setContentsMargins(0, 0, 0, 0);
    top->setSpacing(0);
    mark_session = new Mark;
    mark_seq = new Mark;
    QLabel *title = new QLabel(QString::fromUtf8("\xed\x95\x9c\xea\xb8\x80 Lico session view"));
    title->setAlignment(Qt::AlignCenter);
    top->addWidget(mark_session);
    top->addWidget(mark_seq);
    top->addWidget(title, 1);
    edit = new QPlainTextEdit;
    edit->setReadOnly(true);
    edit->setMaximumBlockCount(VISIBLE_LINES);
    edit->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    lines_in_buf = 0;
    column->addLayout(top);
    column->addWidget(edit, 1);
    entry = new InputLine;
    QObject::connect(entry, &QLineEdit::textChanged, [](const QString &t) { emit_ime("text", t); });
    QObject::connect(entry, &QLineEdit::returnPressed, []() { send_draft(); });
    column->addWidget(entry);
    root->addWidget(view, 1);
    placeholder->hide();
    view->show();
    entry->setFocus();
}

static void hide_view()
{
    int known = cur_session.empty() ? -1 : meta_find(cur_session);
    if (known >= 0) meta_cursor[known] = cursor;
    if (view && entry && !cur_session.empty()) {
        /* a composition in progress is committed to the session it was typed for: reset the
           input method and let its answer land in this entry before the entry goes away */
        QString pending = last_preedit;
        if (!pending.isEmpty()) {
            commit_seen = false;
            QGuiApplication::inputMethod()->reset();
            qint64 until = now_ms() + 250;
            while (!commit_seen && now_ms() < until) QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
            if (commit_seen) pending.clear();      /* the text is in the entry now */
            else { stale_pending = true; stale_empty = false; } /* no answer yet: its late commit is ignored */
        }
        drafts[cur_session] = (entry->text() + pending).toUtf8().constData();
        last_preedit.clear();
    }
    if (!view_id.empty()) {
        send_line("{\"op\":\"view.detach\",\"view\":\"" + view_id + "\"}");
        view_id.clear();
    } else if (subscribed && !cur_session.empty()) {
        send_line("{\"op\":\"unsubscribe\",\"session\":\"" + cur_session + "\"}");
    }
    subscribed = false;
    if (view) {
        delete view; // releases the view tree, text document and marker widgets
        view = nullptr;
        edit = nullptr;
        entry = nullptr;
        mark_session = mark_seq = nullptr;
        lines_in_buf = 0;
        placeholder->show();
    }
}

static void show_session(const std::string &id, long long after, bool has_after)
{
    hide_view();
    cur_session = id;
    int known = meta_find(id);
    if (has_after) cursor = after;
    else if (known >= 0) cursor = meta_cursor[known];
    else cursor = -1;
    build_view();
    mark_session->set((unsigned)strtoul(id.substr(1, 6).c_str(), nullptr, 16) & 0xffffff);
    auto draft = drafts.find(id);
    if (draft != drafts.end()) { entry->setText(QString::fromUtf8(draft->second.c_str())); drafts.erase(draft); }
    char msg[240];
    if (has_after)
        snprintf(msg, sizeof msg, "{\"op\":\"subscribe\",\"session\":\"%s\",\"after\":%lld}", id.c_str(), cursor);
    else /* a view is registered with the host and gets the latest screen first */
        snprintf(msg, sizeof msg, "{\"op\":\"view.attach\",\"session\":\"%s\",\"kind\":\"gui\",\"after\":%lld}", id.c_str(), cursor);
    send_line(msg);
}

static void on_snapshot_lines(const char *reply)
{
    QJsonObject snapshot = QJsonDocument::fromJson(QByteArray(reply)).object().value("snapshot").toObject();
    if (snapshot.value("text").toBool()) {
        for (const QJsonValue &entry : snapshot.value("lines").toArray()) {
            QJsonArray pair = entry.toArray();
            if (pair.size() == 2) append_text((long long)pair.at(0).toDouble(), pair.at(1).toString());
        }
        return;
    }
    const char *p = strstr(reply, "\"lines\":[");
    if (!p) return;
    p += 9;
    while ((p = strchr(p, '[')) != nullptr) {
        char *q;
        long long seq = strtoll(p + 1, &q, 10);
        const char *s = strchr(q, '"');
        if (!s) break;
        s++;
        const char *e = strchr(s, '"');
        if (!e) break;
        size_t full = (size_t)(e - s);
        std::string hex(full > 9 ? std::string(s + 9, std::min(full - 9, (size_t)LINE_HEX)) : std::string());
        append_line(seq, hex);
        p = e;
    }
}

/* a reply to one of our inputs carries its request id: a refused input returns to its draft */
static bool on_input_reply(const char *line)
{
    std::string id;
    if (!jstr(line, "id", &id) || id.rfind("gui-", 0) != 0) return false;
    auto pending = pending_inputs.find(id);
    if (pending != pending_inputs.end()) {
        if (strstr(line, "\"ok\":0")) {
            const std::string session = pending->second.first, text = pending->second.second;
            if (entry && cur_session == session) entry->setText(QString::fromUtf8(text.c_str()) + entry->text());
            else drafts[session] = text + drafts[session];
            emit_ime("send_failed", QString::fromUtf8(text.c_str()));
        }
        pending_inputs.erase(pending);
    }
    return true;
}

static void handle_sock_line(const char *line)
{
    std::string ev, sess, data;
    long long seq;
    if (on_input_reply(line)) return;
    if (jstr(line, "ev", &ev) && ev == "rec") {
        if (!view || !jstr(line, "session", &sess) || sess != cur_session || !jint(line, "seq", &seq)) return;
        if (seq <= cursor) { dups++; return; }
        if (cursor >= 0 && seq != cursor + 1) gaps++;
        if (strstr(line, "\"text\":")) {
            QJsonObject rec = QJsonDocument::fromJson(QByteArray(line)).object();
            if (rec.value("text").isString()) append_text(seq, rec.value("text").toString());
        } else if (jstr(line, "data", &data)) append_line(seq, data);
        return;
    }
    if (jstr(line, "ev", &ev) && ev == "overflow") {
        if (!view || !jstr(line, "session", &sess) || sess != cur_session) return; /* not our visible session */
        subscribed = false;
        resyncs++;
        emit_state("overflow");
        char again[200];
        snprintf(again, sizeof again, "{\"op\":\"subscribe\",\"session\":\"%s\",\"after\":%lld,\"snapshot\":true}",
                 cur_session.c_str(), cursor);
        send_line(again);
        return;
    }
    if (strstr(line, "\"ok\":1") && strstr(line, "\"gap\":")) {
        subscribed = true;
        std::string vid;
        if (jstr(line, "view", &vid)) view_id = vid;
        long long gen;
        if (jint(line, "generation", &gen)) generation = gen;
        if (strstr(line, "\"snapshot\":") && view) {
            edit->clear();
            lines_in_buf = 0;
            on_snapshot_lines(line);
            long long snap;
            const char *sp = strstr(line, "\"snapshot\":");
            if (sp && jint(sp, "seq", &snap)) cursor = snap;
        }
        emit_state("subscribed");
    } else if (strstr(line, "\"sessions\":[")) {
        on_status_reply(line);
        if (print_status) { printf("%s\n", line); fflush(stdout); }
        print_status = false;
        emit_state("loaded");
    }
}

static void handle_command(const std::string &line)
{
    char id[40];
    long long after;
    if (sscanf(line.c_str(), "show %39s %lld", id, &after) == 2) show_session(id, after, true);
    else if (sscanf(line.c_str(), "show %39s", id) == 1) show_session(id, -1, false);
    else if (line.rfind("hide", 0) == 0) { hide_view(); emit_state("hidden"); }
    else if (line.rfind("status", 0) == 0) emit_state("status");
    else if (line.rfind("lines", 0) == 0) {
        QJsonArray shown;
        if (edit) for (const QString &l : edit->toPlainText().split('\n')) shown.append(l);
        QJsonObject out{{"ev", "lines"}, {"lines", shown}};
        printf("%s\n", QJsonDocument(out).toJson(QJsonDocument::Compact).constData());
        fflush(stdout);
    }
    else if (line.rfind("text", 0) == 0) emit_ime("text", entry ? entry->text() : QString());
    else if (line.rfind("hoststatus", 0) == 0) { print_status = true; send_line("{\"op\":\"status\"}"); }
    else if (line.rfind("load", 0) == 0) send_line("{\"op\":\"status\"}");
    else if (line.rfind("quit", 0) == 0) QApplication::quit();
}

template <typename F> static void drain(int fd, std::string &buffer, F handle)
{
    char buf[8192];
    ssize_t n = read(fd, buf, sizeof buf);
    if (n <= 0) { QApplication::quit(); return; }
    buffer.append(buf, (size_t)n);
    size_t nl;
    while ((nl = buffer.find('\n')) != std::string::npos) {
        handle(buffer.substr(0, nl));
        buffer.erase(0, nl + 1);
    }
}

int main(int argc, char **argv)
{
    const char *path = nullptr;
    for (int i = 1; i + 1 < argc; i++)
        if (!strcmp(argv[i], "--socket")) path = argv[i + 1];
    if (!path) { fprintf(stderr, "usage: candidate-c --socket PATH\n"); return 2; }
    QApplication app(argc, argv);
    sockaddr_un addr{};
    addr.sun_family = AF_UNIX;
    snprintf(addr.sun_path, sizeof addr.sun_path, "%s", path);
    sock_fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (connect(sock_fd, (sockaddr *)&addr, sizeof addr) != 0) { perror("connect"); return 1; }
    window = new QWidget;
    window->resize(1200, 800);
    window->setWindowTitle("Lico candidate C");
    root = new QVBoxLayout(window);
    root->setContentsMargins(0, 0, 0, 0);
    placeholder = new QLabel("no session view");
    placeholder->setAlignment(Qt::AlignCenter);
    root->addWidget(placeholder, 1);
    window->show();
    QSocketNotifier sock_notifier(sock_fd, QSocketNotifier::Read);
    QObject::connect(&sock_notifier, &QSocketNotifier::activated, [](QSocketDescriptor, QSocketNotifier::Type) {
        drain(sock_fd, sock_in, [](const std::string &l) { handle_sock_line(l.c_str()); });
    });
    QSocketNotifier stdin_notifier(0, QSocketNotifier::Read);
    QObject::connect(&stdin_notifier, &QSocketNotifier::activated, [](QSocketDescriptor, QSocketNotifier::Type) {
        drain(0, cmd_in, [](const std::string &l) { handle_command(l); });
    });
    printf("{\"ev\":\"ready\",\"pid\":%d}\n", (int)getpid());
    fflush(stdout);
    return app.exec();
}
