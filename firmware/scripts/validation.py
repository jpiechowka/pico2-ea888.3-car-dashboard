"""Non-removable predicates for build and source policy gates."""


def require(condition: object, message: str = "validation failed") -> None:
    if not condition:
        raise AssertionError(message)
