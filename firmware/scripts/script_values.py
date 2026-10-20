"""Checked object boundaries for script inputs; annotations never admit data."""


def object_fields(value: object, message: str) -> dict[str, object]:
    if type(value) is not dict:
        raise AssertionError(message)
    result: dict[str, object] = {}
    for key, member in value.items():
        if type(key) is not str:
            raise AssertionError(message)
        result[key] = member
    return result


def array_items(value: object, message: str) -> list[object]:
    if type(value) is not list:
        raise AssertionError(message)
    return list(value)


def string_value(value: object, message: str) -> str:
    if type(value) is not str:
        raise AssertionError(message)
    return value


def integer_value(value: object, message: str) -> int:
    # bool is a Python int subclass but is a different wire scalar.
    if type(value) is not int:
        raise AssertionError(message)
    return value
