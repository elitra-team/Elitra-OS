#ifndef ELITRA_LIB_HPP
#define ELITRA_LIB_HPP

#include <cstddef>
#include <cstdint>

namespace lib {

void *memset(void *s, int c, size_t n);
void *memcpy(void *dest, const void *src, size_t n);
void *memmove(void *dest, const void *src, size_t n);
int   memcmp(const void *s1, const void *s2, size_t n);

size_t strlen(const char *s);
int    strcmp(const char *s1, const char *s2);
int    strncmp(const char *s1, const char *s2, size_t n);
char  *strcpy(char *dest, const char *src);
char  *strncpy(char *dest, const char *src, size_t n);
char  *strcat(char *dest, const char *src);
char  *strchr(const char *s, int c);

void itoa(int num, char *str, int base);
void uitoa(uint32_t num, char *str, int base);
void uitoa64(uint64_t num, char *str, int base);
int  atoi(const char *str);

}

#endif
