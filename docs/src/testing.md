# Testing

There are two distinct test suites: One for unit test (`just test-unit`) and
integration tests (`just test-integration`) that is part of the rust crate, and
a separate e2e test suite in python (`just test-e2e`).

To run all tests, run `just test`.

When contributing, consider whether it makes sense to add tests which could
prevent regressions in the future. When fixing bugs, it makes sense to add tests
that expose the wrong behavior beforehand.

The unit and integration tests are very small and only test a few self-contained
functions (like validation of certain input).

## E2E tests

The main focus of the testing setup lays on the e2e tests. Each user-facing
behavior *should* have a corresponding e2e test. These are the most important
tests, as they test functionality the user will use in the end.

The test suite is written in python and uses
[pytest](https://docs.pytest.org/en/stable/). There are helper functions that
set up temporary git repositories and remotes in a `tmpfs`.

Effectively, each tests works like this:

* Set up some prerequisites (e.g. different git repositories or configuration
  files)
* Run `grm`
* Check that everything is according to expected behavior (e.g. that `grm` had
  certain output and exit code, that the target repositories have certain
  branches, heads and remotes, ...)

As there are many different scenarios, the tests make heavy use of the
[`@pytest.mark.parametrize`](https://docs.pytest.org/en/stable/how-to/parametrize.html#pytest-mark-parametrize)
decorator to get all permutations of input parameters (e.g. whether a
configuration exists, what a config value is set to, how the repository looks
like, ...)

Whenever you write a new test, think about the different circumstances that can
happen. What are the failure modes? What affects the behavior? Parametrize each
of these behaviors.

### Optimization

Note: You will most likely not need to read this.

Each test parameter will exponentially increase the number of tests that will be
run. As a general rule, comprehensiveness is more important than test suite
runtime (so if in doubt, better to add another parameter to catch every edge
case). But try to keep the total runtime sane. Currently, the whole `just test-e2e`
target runs ~8'000 tests and takes around 5 minutes on my machine, exlucding
binary and podman build time. I'd say that keeping it under 10 minutes is a good
idea.

To optimize tests, look out for two patterns: Dependency and Orthogonality

#### Dependency

If a parameter depends on another one, it makes little sense to handle them
independently. Example: You have a paramter that specifies whether a
configuration is used, and another parameter that sets a certain value in that
configuration file. It might look something like this:

```python
@pytest.mark.parametrize("use_config", [True, False])
@pytest.mark.parametrize("use_value", ["0", "1"])
def test(...):
```

This leads to 4 tests being instantiated. But there is little point in setting a
configuration value when no config is used, so the combinations `(False, "0")`
and `(False, "1")` are redundant. To remedy this, spell out the optimized
permutation manually:

```python
@pytest.mark.parametrize("config", ((True, "0"), (True, "1"), (False, None)))
def test(...):
    (use_config, use_value) = config
```

This cuts down the number of tests by 25%. If you have more dependent parameters
(e.g. additional configuration values), this gets even better.  Generally, this
will cut down the number of tests to

\\[ \frac{1}{o \cdot c} + \frac{1}{(o \cdot c) ^ {(n + 1)}} \\]

with \\( o \\) being the number of values of a parent parameters a parameter is
dependent on, \\( c \\) being the cardinality of the test input (so you can
assume \\( o = 1 \\) and \\( c = 2 \\) for boolean parameters), and \\( n \\)
being the number of parameters that are optimized, i.e. folded into their
dependent parameter.

As an example: Folding down two boolean parameters into one dependent parent
boolean parameter will cut down the number of tests to 62.5%!

#### Orthogonality

If different test parameters are independent of each other, there is little
point in testing their combinations. Instead, split them up into different test
functions. For boolean parameters, this will cut the number of tests in half.

So instead of this:

```python
@pytest.mark.parametrize("param1", [True, False])
@pytest.mark.parametrize("param2", [True, False])
def test(...):
```

Rather do this:

```python
@pytest.mark.parametrize("param1", [True, False])
def test_param1(...):

@pytest.mark.parametrize("param2", [True, False])
def test_param2(...):
```

The tests are running in podman via podman-compose. This is mainly needed to
test networking functionality like GitLab integration, with the GitLab API being
mocked by a simple flask container.
