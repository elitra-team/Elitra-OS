#ifndef ELITRA_SHELL_HPP
#define ELITRA_SHELL_HPP

#include <cstdint>

namespace kernel {

class Shell {
public:
    void run();

private:
    static const int MAX_ARGS   = 16;
    static const int MAX_CMD_LEN = 256;

    static bool cpuid_supported();
    static void get_cpuid(int code, uint32_t *a, uint32_t *b, uint32_t *c, uint32_t *d);

    void cmd_help();
    void cmd_clear();
    void cmd_echo(char **args, int argc);
    void cmd_uptime();
    void cmd_meminfo();
    void cmd_cpuinfo();
    void cmd_version();
    void cmd_reboot();
    void cmd_shutdown();
    void cmd_tasks();
    void cmd_testmalloc();
    void cmd_testpaging();
    void cmd_createtask();
    void cmd_ls(char *args);
    void cmd_cat(char *args);
    void cmd_vfsinfo();
    void cmd_touch(char *args);
    void cmd_rm(char *args);
    void cmd_mkdir(char *args);
    void cmd_write(char **args, int argc);
    void cmd_mount(char *args);
    void cmd_umount(char *args);
    void cmd_exec(char *args);
    void cmd_ata(char *args);
    void cmd_sync(char *args);

    void parse_args(char *cmd, char **args, int *argc);
};

}

#endif
