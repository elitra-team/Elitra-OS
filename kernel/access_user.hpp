#ifndef ELITRA_ACCESS_USER_HPP
#define ELITRA_ACCESS_USER_HPP

#include <cstdint>

static const uint64_t USER_ADDR_MAX = 0x00007FFFFFFFFFFFULL;

static inline bool is_user_range(const void *ptr, uint64_t size) {
    uint64_t addr = reinterpret_cast<uint64_t>(ptr);
    if (addr + size < addr) return false;
    if (addr + size > USER_ADDR_MAX) return false;
    if (addr < 0x1000) return false;
    return true;
}

static inline int copy_from_user(void *dst, const void *src, uint64_t size) {
    if (!is_user_range(src, size)) return -1;
    __builtin_memcpy(dst, src, size);
    return 0;
}

static inline int copy_to_user(void *dst, const void *src, uint64_t size) {
    if (!is_user_range(dst, size)) return -1;
    __builtin_memcpy(dst, src, size);
    return 0;
}

static inline int copy_string_from_user(char *dst, const char *src, uint64_t max_len) {
    if (!is_user_range(src, 1)) return -1;
    for (uint64_t i = 0; i < max_len; i++) {
        dst[i] = src[i];
        if (dst[i] == '\0') return static_cast<int>(i);
        if (!is_user_range(&src[i], 1)) {
            dst[i] = '\0';
            return -1;
        }
    }
    dst[max_len - 1] = '\0';
    return static_cast<int>(max_len - 1);
}

#endif
