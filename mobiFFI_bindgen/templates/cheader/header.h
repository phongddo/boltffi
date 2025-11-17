#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

typedef struct { int32_t code; } FfiStatus;
typedef struct { uint8_t* ptr; size_t len; size_t cap; } FfiString;
FfiStatus {{ prefix }}_last_error_message(FfiString* out);
void {{ prefix }}_clear_last_error(void);
{%- for record in records %}

typedef struct {
{%- for field in record.fields %}
    {{ field.c_type }} {{ field.name }};
{%- endfor %}
} {{ record.name }};
{%- endfor %}
{% for func in functions %}
{{ func.signature }};
{%- endfor %}

void {{ prefix }}_free_string(FfiString s);
