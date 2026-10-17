# Questions

The /spec loop writes a question here when the spec does not decide something a round needs, and skips that item until the question has an answer. Write the answer after **Answer:**, in your own words or as the number of an option, then run /spec. An answered question is part of the spec: every later round of the item reads it, and changing the answer rebuilds the item.

## §05-cut#1-screen · This item did not pass its attempts. How should the rounds go on?

Asked 2026-10-17 by codehalter v138 (2026-10-17, f014493) while building 1. Screen.

The spec comes closest in `05-cut.md:11`:

> 1. Screen

What stopped the last attempt:

```
the test command did not pass. The end of its output:

` ` `
…ffers_the_project_for_naming ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s

     Running tests/sd_cancel_on_stop.rs (target/debug/deps/sd_cancel_on_stop-c54f59be36323c4d)

running 3 tests
test sec_02_services_1_the_four_servers_s12_a_refused_cancel_names_the_drawing_server ... ok
test sec_02_services_1_the_four_servers_s12_a_cancel_posts_the_job_cancel_path ... ok
test sec_02_services_1_the_four_servers_s12_a_stop_during_the_poll_cancels_the_job ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.31s

     Running tests/seam_retries.rs (target/debug/deps/seam_retries-a9ecbc39c2693a85)

running 6 tests
test p_machine_seamretries_s1_default_one_makes_two_asks ... ok
test p_machine_seamretries_s3_log_tail_says_whether_it_will_ask_again ... ok
test p_machine_seamretries_s4_the_join_cannot_be_asked_past_the_bound ... ok
test p_machine_seamretries_s5_a_refusal_removes_nothing ... ok
test p_machine_seamretries_s2_each_ask_keys_elsewhere ... ok
test p_machine_seamretries_s6_family_is_machine_and_the_row_is_deliberately_unrowed ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/sec05_cut_2_model_base.rs (target/debug/deps/sec05_cut_2_model_base-727a28f53fa5ec8a)

running 2 tests
libEGL warning: DRI3 error: Could not get DRI3 device
libEGL warning: Ensure your X server supports DRI3 to get accelerated rendering
test sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s1_a_suggestion_becomes_what_revert_goes_back_to ... ok
test sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s2_a_revert_after_a_suggestion_keeps_the_suggested_work_and_drops_only_hand_edits ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s

     Running tests/sec05_cut_2_session_clock.rs (target/d
```

Options:

1. Try again as the spec says. Example: right when the cause was elsewhere and is gone now, such as a build another item broke or a tool installed since: answer `1`, and the next /spec gives it fresh attempts
2. Change what the spec asks. Example: edit the item's section in `spec/05-cut.md` (a smaller step, another format, a service left out) and answer `2`; the next /spec builds the new text
3. Tell the rounds how to get past it. Example: answer `3: test the upload against a local fake server on port 8080` or `3: install libadwaita 1.5 in the Dockerfile first`, and every round of the item reads it

**Answer:** 
