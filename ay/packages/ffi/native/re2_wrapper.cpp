// re2_wrapper.cpp — thin C API over RE2.
// Compiled at first import by @aphrody/ay-ffi/regex-re2 (src/native.ts, g++).
// TinyCC (bun:cc) does not support C++, hence the g++-built .so approach.

#include <re2/re2.h>
#include <string.h>
#include <stdlib.h>
#include <stdint.h>
#include <vector>

extern "C" {

// ---- lifecycle ---------------------------------------------------------

// Allocate and compile a RE2 pattern.
void* re2_new(const char* pattern) {
    if (!pattern) return nullptr;
    return new RE2(pattern);
}

void re2_free(void* re) {
    if (re) delete static_cast<RE2*>(re);
}

int re2_ok(void* re) {
    if (!re) return 0;
    return static_cast<RE2*>(re)->ok() ? 1 : 0;
}

const char* re2_error(void* re) {
    if (!re) return "null RE2 pointer";
    return static_cast<RE2*>(re)->error().c_str();
}

const char* re2_pattern(void* re) {
    if (!re) return "";
    return static_cast<RE2*>(re)->pattern().c_str();
}

// ---- matching ----------------------------------------------------------

int re2_full_match(void* re, const char* text) {
    if (!re || !text) return 0;
    return RE2::FullMatch(text, *static_cast<RE2*>(re)) ? 1 : 0;
}

int re2_partial_match(void* re, const char* text) {
    if (!re || !text) return 0;
    return RE2::PartialMatch(text, *static_cast<RE2*>(re)) ? 1 : 0;
}

int re2_num_groups(void* re) {
    if (!re) return 0;
    return static_cast<RE2*>(re)->NumberOfCapturingGroups();
}

// ---- exec: returns groups as a single packed blob ----------------------
//
// Layout of the returned buffer (heap-allocated, caller frees with re2_free_exec):
//   [uint32_t count][uint32_t off0][uint32_t len0]...[uint32_t offN][uint32_t lenN][...string data...]
//
// Returns NULL on no match.
//
// This avoids char** pointer-in-pointer complexity that bun:ffi doesn't support.

struct ExecResult {
    uint32_t count;
    // variable: pairs (offset, length) then the string bytes back-to-back
};

void* re2_exec(void* re_ptr, const char* text) {
    if (!re_ptr || !text) return nullptr;
    RE2* re = static_cast<RE2*>(re_ptr);

    int n = re->NumberOfCapturingGroups();
    if (n <= 0) {
        // No groups: just test partial match
        return RE2::PartialMatch(text, *re) ? (void*)1 : nullptr; // sentinel
    }
    if (n > 32) n = 32;

    std::vector<std::string> results(n);
    std::vector<RE2::Arg> args(n);
    std::vector<RE2::Arg*> arg_ptrs(n);
    for (int i = 0; i < n; i++) {
        args[i] = &results[i];
        arg_ptrs[i] = &args[i];
    }

    if (!RE2::PartialMatchN(text, *re, arg_ptrs.data(), n)) return nullptr;

    // Compute total size
    size_t data_size = 0;
    for (int i = 0; i < n; i++) data_size += results[i].size();
    size_t header_size = sizeof(uint32_t) * (1 + 2 * n); // count + (off,len)*n
    uint8_t* buf = (uint8_t*)malloc(header_size + data_size);
    if (!buf) return nullptr;

    uint32_t* h = (uint32_t*)buf;
    h[0] = (uint32_t)n;
    uint8_t* data_ptr = buf + header_size;
    uint32_t offset = 0;
    for (int i = 0; i < n; i++) {
        h[1 + 2*i]     = offset;
        h[1 + 2*i + 1] = (uint32_t)results[i].size();
        memcpy(data_ptr + offset, results[i].c_str(), results[i].size());
        offset += (uint32_t)results[i].size();
    }
    return buf;
}

void re2_free_exec(void* buf, int n) {
    // Sentinel value (1) means "no groups but matched": nothing to free
    if (!buf || buf == (void*)1) return;
    free(buf);
}

} // extern "C"
