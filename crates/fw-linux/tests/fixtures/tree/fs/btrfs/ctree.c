#include "ctree.h"

int btrfs_ctree_search(struct file *file)
{
	return btrfs_super_init() + vfs_read(file, 0, 0, 0);
}
