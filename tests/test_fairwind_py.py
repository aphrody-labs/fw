import unittest
from pathlib import Path


class FairwindPythonLayoutTest(unittest.TestCase):
    def test_heretic_absorbed_under_core_py(self) -> None:
        heretic_dir = Path("core/py/heretic")
        self.assertTrue(heretic_dir.is_dir())
        self.assertGreater(len(list(heretic_dir.rglob("*.py"))), 0)


if __name__ == "__main__":
    unittest.main()
