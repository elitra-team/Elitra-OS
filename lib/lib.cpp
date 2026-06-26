#include "lib.hpp"

extern "C" {
void *krust_memcpy(void *dest, const void *src, size_t n);
void *krust_memmove(void *dest, const void *src, size_t n);
void *krust_memset(void *s, int c, size_t n);
int   krust_memcmp(const void *s1, const void *s2, size_t n);
size_t krust_strlen(const char *s);
int    krust_strcmp(const char *s1, const char *s2);
int    krust_strncmp(const char *s1, const char *s2, size_t n);
char  *krust_strcpy(char *dest, const char *src);
char  *krust_strncpy(char *dest, const char *src, size_t n);
char  *krust_strcat(char *dest, const char *src);
char  *krust_strchr(const char *s, int c);
void   krust_itoa_base(int num, char *buf, unsigned int base);
void   krust_uitoa_base(unsigned int num, char *buf, unsigned int base);
void   krust_uitoa64_base(uint64_t num, char *buf, unsigned int base);
int    krust_atoi(const char *s);
}

void *lib::memset(void *s, int c, size_t n) { return krust_memset(s, c, n); }
void *lib::memcpy(void *dest, const void *src, size_t n) { return krust_memcpy(dest, src, n); }
void *lib::memmove(void *dest, const void *src, size_t n) { return krust_memmove(dest, src, n); }

// Rust codegen may emit direct memcpy calls; forward to our implementation
extern "C" void *memcpy(void *dest, const void *src, size_t n) { return krust_memcpy(dest, src, n); }
int   lib::memcmp(const void *s1, const void *s2, size_t n) { return krust_memcmp(s1, s2, n); }
size_t lib::strlen(const char *s) { return krust_strlen(s); }
int    lib::strcmp(const char *s1, const char *s2) { return krust_strcmp(s1, s2); }
int    lib::strncmp(const char *s1, const char *s2, size_t n) { return krust_strncmp(s1, s2, n); }
char  *lib::strcpy(char *dest, const char *src) { return krust_strcpy(dest, src); }
char  *lib::strncpy(char *dest, const char *src, size_t n) { return krust_strncpy(dest, src, n); }
char  *lib::strcat(char *dest, const char *src) { return krust_strcat(dest, src); }
char  *lib::strchr(const char *s, int c) { return krust_strchr(s, c); }

void lib::itoa(int num, char *str, int base) { krust_itoa_base(num, str, static_cast<unsigned int>(base)); }
void lib::uitoa(uint32_t num, char *str, int base) { krust_uitoa_base(num, str, static_cast<unsigned int>(base)); }
void lib::uitoa64(uint64_t num, char *str, int base) { krust_uitoa64_base(num, str, static_cast<unsigned int>(base)); }
int  lib::atoi(const char *str) { return krust_atoi(str); }
