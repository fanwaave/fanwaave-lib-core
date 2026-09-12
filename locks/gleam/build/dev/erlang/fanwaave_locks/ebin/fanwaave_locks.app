{application, fanwaave_locks, [
    {vsn, "0.1.0"},
    {applications, [gleam_stdlib,
                    gleeunit,
                    ores_locks_and_leases]},
    {description, "fanwaave lock routines: ores_locks_and_leases with the fanwaave key prefix and lock catalog."},
    {modules, [fanwaave_locks_test]},
    {registered, []}
]}.
