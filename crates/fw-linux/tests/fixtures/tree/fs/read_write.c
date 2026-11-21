// SPDX-License-Identifier: GPL-2.0
#include <linux/fs.h>
#include <linux/mutex.h>

static DEFINE_MUTEX(rw_lock);
DEFINE_PER_CPU(int, rw_counter);
static const struct file_operations rw_fops __ro_after_init = {
	.read = vfs_read_fop,
};

static long vfs_read_fop(struct file *file, char *buf, unsigned long count)
{
	return vfs_read(file, buf, count, &file->f_pos);
}

long vfs_read(struct file *file, char __user *buf, unsigned long count, loff_t *pos)
{
	long size = fs_min(count, FMODE_READ);

	rw_helper(file);
	if (file_pos(file) > 0)
		return -1;
	return size + sysctl_nr_open;
}
EXPORT_SYMBOL(vfs_read);

static int __init rw_helper(struct file *file)
{
	return vfs_read_fop(file, 0, 0);
}

SYSCALL_DEFINE3(read, unsigned int, fd, char __user *, buf, size_t, count)
{
	return ksys_read(fd, buf, count);
}

COMPAT_SYSCALL_DEFINE3(read, unsigned int, fd, char __user *, buf, size_t, count)
{
	return ksys_read(fd, buf, count);
}

SYSCALL_DEFINE0(getpid)
{
	return 1;
}

long ksys_read(unsigned int fd, char __user *buf, size_t count)
{
	return vfs_read(0, buf, count, 0);
}
EXPORT_SYMBOL_GPL(ksys_read);
EXPORT_SYMBOL_NS_GPL(rw_helper, "FIXTURE_NS");

static int __init rw_init(void)
{
	return 0;
}
module_init(rw_init);
