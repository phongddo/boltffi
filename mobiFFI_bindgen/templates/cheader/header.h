#pragma once

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdatomic.h>

typedef struct { int32_t code; } FfiStatus;
typedef struct { uint8_t* ptr; size_t len; size_t cap; } FfiString;
static inline bool {{ prefix }}_atomic_u8_cas(uint8_t* state, uint8_t expected, uint8_t desired) {
    return atomic_compare_exchange_strong_explicit((_Atomic uint8_t*)state, &expected, desired, memory_order_acq_rel, memory_order_acquire);
}
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
