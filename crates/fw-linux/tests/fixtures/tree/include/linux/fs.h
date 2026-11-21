#ifndef _LINUX_FS_H
#define _LINUX_FS_H

#include <linux/types.h>
#include <linux/magic.h>
#include <asm/x.h>
#include <linux/missing.h>
#include "fs_local.h"

#define FMODE_READ 0x1
#define MAX_RW_COUNT(x, y) ((x) < (y) ? (x) : (y))
#define fs_min(a, b) MAX_RW_COUNT(a, b)

enum rw_hint {
	HINT_NONE,
	HINT_SHORT = 2,
	HINT_LONG = HINT_SHORT + 1,
};

struct file;

struct file_operations {
	long (*read)(struct file *file, char *buf, unsigned long count);
	struct list_head link;
	union {
		u32 raw;
		unsigned char bytes[4];
	} flags;
};

struct file {
	const struct file_operations *f_op;
	loff_t f_pos;
};

extern int sysctl_nr_open;
long vfs_read(struct file *file, char *buf, unsigned long count, loff_t *pos);

static __always_inline long file_pos(struct file *file)
{
	return file->f_pos;
}

#endif
