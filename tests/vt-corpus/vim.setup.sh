#!/bin/sh
# Setup for vim.steps: a source file of about 100 lines and a small vimrc, in the working directory.
set -e

cat > sample.c <<'EOF'
/* sample.c: a small hash table, used as text for the vim recording. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TABLE_SIZE 64
#define MAX_KEY 32

typedef struct entry {
    char key[MAX_KEY];
    int value;
    struct entry *next;
} entry_t;

typedef struct table {
    entry_t *buckets[TABLE_SIZE];
    size_t count;
} table_t;

/* The classic djb2 string hash. */
static unsigned long hash(const char *s)
{
    unsigned long h = 5381;
    int c;

    while ((c = (unsigned char)*s++) != 0) {
        h = ((h << 5) + h) + c;
    }
    return h % TABLE_SIZE;
}

static table_t *table_new(void)
{
    table_t *t = calloc(1, sizeof(*t));
    if (t == NULL) {
        perror("calloc");
        exit(EXIT_FAILURE);
    }
    return t;
}

static entry_t *table_find(const table_t *t, const char *key)
{
    entry_t *e = t->buckets[hash(key)];

    while (e != NULL) {
        if (strcmp(e->key, key) == 0) {
            return e;
        }
        e = e->next;
    }
    return NULL;
}

static void table_set(table_t *t, const char *key, int value)
{
    entry_t *e = table_find(t, key);

    if (e != NULL) {
        e->value = value;
        return;
    }
    e = calloc(1, sizeof(*e));
    if (e == NULL) {
        perror("calloc");
        exit(EXIT_FAILURE);
    }
    strncpy(e->key, key, MAX_KEY - 1);
    e->value = value;
    e->next = t->buckets[hash(key)];
    t->buckets[hash(key)] = e;
    t->count++;
}

static int table_get(const table_t *t, const char *key, int fallback)
{
    entry_t *e = table_find(t, key);

    return e != NULL ? e->value : fallback;
}

static void table_free(table_t *t)
{
    for (size_t i = 0; i < TABLE_SIZE; i++) {
        entry_t *e = t->buckets[i];

        while (e != NULL) {
            entry_t *next = e->next;
            free(e);
            e = next;
        }
    }
    free(t);
}

int main(int argc, char **argv)
{
    table_t *t = table_new();
    const char *words[] = { "alpha", "beta", "gamma", "delta", "epsilon" };

    for (int i = 0; i < 5; i++) {
        table_set(t, words[i], i * 10 + 1);
    }
    for (int i = 1; i < argc; i++) {
        printf("%s = %d\n", argv[i], table_get(t, argv[i], -1));
    }
    printf("%zu entries\n", t->count);
    table_free(t);
    return 0;
}
EOF

cat > vimrc <<'EOF'
set nocompatible
syntax on
filetype plugin indent on
set number
set ruler
set showcmd
set mouse=a
set background=dark
set hlsearch
set incsearch
set noswapfile
set nobackup
set viminfo=
EOF
