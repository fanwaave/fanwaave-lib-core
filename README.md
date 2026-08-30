# fanwaave-lib-core

ORM and migration source of truth for Fan and push-notification delivery across web, mobile, and desktop. Web servers may use the read-only capability; reviewed deployment jobs invoke `declarative-migrations` (`dpm`) to plan, verify, and apply schema changes. Runtime crates, including this library and `fanwaave-orm-core`, never apply DDL at startup.
