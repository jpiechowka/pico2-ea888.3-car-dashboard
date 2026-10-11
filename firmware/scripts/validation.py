"""Non-removable predicates for build and source policy gates."""


def require(condition, message="validation failed"):
    if not condition:
        raise AssertionError(message)
