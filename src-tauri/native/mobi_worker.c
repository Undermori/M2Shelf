/* M²Shelf's read-only, isolated libmobi worker. LGPL-3.0-or-later.
 * Only an already-open inherited source handle is accepted. No path lookup,
 * extraction, network, fonts, encryption or external commands are used. */
#include "mobi.h"
/* M2Shelf worker integration. SPDX-License-Identifier: LGPL-3.0-or-later */
#include <stdio.h>
#include "index.h"
#include "parse_rawml.h"
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#include <windows.h>
#else
#include <unistd.h>
#endif

#define MAX_TEXT (32u * 1024u * 1024u)
#define MAX_OUTPUT (96u * 1024u * 1024u)
#define MAX_PARTS 20000u

static int number(uint64_t n) {
    unsigned char bytes[8];
    for (int i = 0; i < 8; ++i) bytes[i] = (unsigned char)(n >> (8 * i));
    return fwrite(bytes, 1, 8, stdout) == 8;
}
static int emit(MOBIPart *part, unsigned kind, size_t *total, size_t *count) {
    for (; part; part = part->next) {
        if (kind == 2 && part->type != T_JPG && part->type != T_PNG &&
            part->type != T_GIF && part->type != T_BMP) continue;
        if (++*count > MAX_PARTS || part->size > MAX_OUTPUT - *total) return 0;
        *total += part->size;
        if (!number(kind) || !number(part->uid) || !number(part->type) ||
            !number(part->size) || fwrite(part->data, 1, part->size, stdout) != part->size) return 0;
    }
    return 1;
}
static int emit_toc(const MOBIRawml *raw, size_t *total, size_t *count) {
    if (!raw->ncx || !raw->ncx->cncx_record) return 1;
    if (raw->ncx->entries_count > 10000) return 0;
    for (size_t i = 0; i < raw->ncx->entries_count; ++i) {
        const MOBIIndexEntry *entry = &raw->ncx->entries[i];
        uint32_t cncx, file = 0, position, offset;
        char target[MOBI_ATTRNAME_MAXSIZE + 1] = {0};
        unsigned text_tag[] = {3, 0}, fid_tag[] = {6, 0}, off_tag[] = {6, 1}, pos_tag[] = {1, 0};
        if (mobi_get_indxentry_tagvalue(&cncx, entry, text_tag) != MOBI_SUCCESS) continue;
        char *text = mobi_get_cncx_string_utf8(raw->ncx->cncx_record, cncx, raw->ncx->encoding);
        if (!text) continue;
        if (mobi_is_rawml_kf8(raw)) {
            MOBIAttrType attr = ATTR_ID;
            if (mobi_get_indxentry_tagvalue(&position, entry, fid_tag) != MOBI_SUCCESS ||
                mobi_get_indxentry_tagvalue(&offset, entry, off_tag) != MOBI_SUCCESS ||
                mobi_get_id_by_posoff(&file, target, raw, position, offset, &attr) != MOBI_SUCCESS) { free(text); continue; }
            if (!offset) target[0] = 0;
        } else {
            if (mobi_get_indxentry_tagvalue(&position, entry, pos_tag) != MOBI_SUCCESS) { free(text); continue; }
            snprintf(target, sizeof(target), "%010u", position);
        }
        size_t a = strlen(target) + 1, b = strlen(text), size = a + b;
        if (b > 4096 || ++*count > MAX_PARTS || size > MAX_OUTPUT - *total) { free(text); return 0; }
        *total += size;
        int ok = number(3) && number(file) && number(0) && number(size) &&
            fwrite(target, 1, a, stdout) == a && fwrite(text, 1, b, stdout) == b;
        free(text);
        if (!ok) return 0;
    }
    return 1;
}
int main(int argc, char **argv) {
    if (argc != 2 || strcmp(argv[1], "--read-stdio-v1")) return 2;
    /* Parent configures the Job before releasing this gate. The read-only book
     * handle occupies stderr, carried by the standard explicit handle list. */
    if (getchar() != 'S') return 2;
#ifdef _WIN32
    FILE *file = _fdopen(_dup(_fileno(stderr)), "rb");
    if (file) _setmode(_fileno(file), _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);
#else
    FILE *file = fdopen(dup(fileno(stderr)), "rb");
#endif
    if (!file) return 3;
    MOBIData *m = mobi_init();
    if (!m) { fclose(file); return 4; }
    MOBI_RET status = mobi_load_file(m, file);
    fclose(file);
    if (status != MOBI_SUCCESS) { mobi_free(m); return 5; }
    if (mobi_is_encrypted(m) || (m->next && mobi_is_encrypted(m->next))) { mobi_free(m); return 10; }
    if (mobi_is_replica(m) || mobi_is_dictionary(m)) { mobi_free(m); return 11; }
    if (mobi_get_text_maxsize(m) > MAX_TEXT || !m->rh || m->rh->text_record_count > 8192) {
        mobi_free(m); return 12;
    }
    MOBIRawml *raw = mobi_init_rawml(m);
    if (!raw) { mobi_free(m); return 4; }
    status = mobi_parse_rawml_opt(raw, m, true, false, true);
    if (status != MOBI_SUCCESS) { mobi_free_rawml(raw); mobi_free(m); return 6; }
    uint64_t cover = UINT64_MAX;
    MOBIExthHeader *exth = mobi_get_exthrecord_by_tag(m, EXTH_COVEROFFSET);
    if (exth) cover = mobi_decode_exthvalue(exth->data, exth->size);
    size_t total = 0, count = 0;
    int ok = fwrite("M2MOBI01", 1, 8, stdout) == 8 && number(raw->version) && number(cover) &&
        emit(raw->markup, 1, &total, &count) && emit(raw->resources, 2, &total, &count) && emit_toc(raw, &total, &count) &&
        number(0) && fflush(stdout) == 0;
    mobi_free_rawml(raw);
    mobi_free(m);
    return ok ? 0 : 12;
}
