// SPDX-License-Identifier: GPL-2.0
#include <linux/fs.h>

struct idiom_entry {
	struct list_head node;
	int value;
};

static struct file_operations idiom_fops = {
	.read = idiom_read,
#ifdef CONFIG_IDIOM
	.llseek = idiom_llseek,
#endif
};

static long idiom_read(struct file *file, char *buf, unsigned long count)
{
	struct idiom_entry *entry;
	int total = 0;

	list_for_each_entry(entry, &idiom_list, node)
		total += container_of(file, struct idiom_entry, node)->value;
	for_each_online_cpu(cpu) {
		total += per_cpu(idiom_counter, cpu);
	}
	return total;
}

static long idiom_after(void)
{
	return 0;
}

SYSCALL_DEFINE2(idiom,
		int, fd,
		const char __user *, name)
{
	return idiom_after();
}

module_init(idiom_boot)
