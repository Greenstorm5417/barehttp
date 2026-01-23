// HttpClient unit tests removed - behaviors are tested via:
// - Policy module tests (src/client/tests/test_policy.rs)
// - Transport module tests (src/transport/tests/)
// - Integration tests (tests/config_test.rs, tests/httpbin_test.rs)
//
// Mock-based unit tests don't work with connection pooling since
// sockets are created internally by the pool.
