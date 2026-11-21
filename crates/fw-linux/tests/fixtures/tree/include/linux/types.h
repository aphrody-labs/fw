#ifndef _LINUX_TYPES_H
#define _LINUX_TYPES_H

typedef unsigned int u32;
typedef long loff_t;
typedef int (*callback_t)(int code);

struct list_head {
	struct list_head *next, *prev;
};

#endif
