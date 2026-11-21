import { describe, expect, test } from "bun:test";
import { bestForWork, parseDf } from "./disks.ts";

const DF = `Filesystem                     Type        1B-blocks         Used        Avail Mounted on
/dev/sda1                      ext4     311422550016 258772480000  50466893824 /
/dev/sda13                     ext4       1036967936    213913600    754077696 /boot
tmpfs                          tmpfs      1234567890            0   1234567890 /run
/dev/mapper/xbuild-data        xfs      864354435072 123481473024 740872962048 /srv/xbuild
10.200.77.1:/srv/xbuild/shared nfs4     864354435072 123481473024 740872962048 /srv/xbuild/shared
`;

describe("disk inventory", () => {
  test("df parsing keeps real filesystems and drops tmpfs", () => {
    const m = parseDf(DF);
    expect(m.map(x => x.target)).toEqual(["/", "/boot", "/srv/xbuild", "/srv/xbuild/shared"]);
    expect(m[2]).toEqual({
      source: "/dev/mapper/xbuild-data",
      fstype: "xfs",
      size: 864354435072,
      used: 123481473024,
      avail: 740872962048,
      target: "/srv/xbuild",
    });
  });

  test("large work goes to the mount with the most free space, never boot", () => {
    expect(bestForWork(parseDf(DF))?.target).toBe("/srv/xbuild");
    expect(bestForWork(parseDf(DF).filter(m => !m.target.startsWith("/srv")))?.target).toBe("/");
  });
});
