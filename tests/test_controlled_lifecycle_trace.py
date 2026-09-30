import unittest

from tools.check_controlled_lifecycle_trace import inspect_trace


class LifecycleTraceTests(unittest.TestCase):
    BEGIN = '100 write(2, "CONTROLLED_MEDIA_BEGIN\\n", 23) = 23\n'
    END = '100 write(2, "CONTROLLED_MEDIA_END\\n", 21) = 21\n'

    def test_harness_creation_outside_markers_is_allowed(self):
        text = '99 clone(...) = 100\n' + self.BEGIN + self.END + '99 clone3(...) = 101\n'
        self.assertEqual(inspect_trace(text, 1), 1)

    def test_creation_and_socket_attempts_inside_markers_are_rejected(self):
        for operation in ['clone', 'clone3', 'fork', 'vfork', 'execve', 'socket', 'connect', 'bind', 'sendto', 'recvfrom']:
            with self.subTest(operation=operation), self.assertRaises(ValueError):
                inspect_trace(self.BEGIN + f'100 {operation}(...) <unfinished ...>\n' + self.END, 1)

    def test_missing_or_unbalanced_markers_are_rejected(self):
        for text in ['', self.BEGIN, self.END, self.BEGIN + self.BEGIN + self.END]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                inspect_trace(text, 1)

    def test_all_expected_scopes_are_required(self):
        with self.assertRaises(ValueError):
            inspect_trace(self.BEGIN + self.END, 2)
        self.assertEqual(inspect_trace((self.BEGIN + self.END) * 9), 9)


if __name__ == '__main__':
    unittest.main()
