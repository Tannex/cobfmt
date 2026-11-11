       >>source format free
identification division.
program-id. accounts.
data division.
working-storage section.
01 input-record.
   05 cust-id       pic 99   value 42.
   05 customer-name pic x(3) value "Ada".
01 output-record.
   05 cust-id       pic 99.
   05 customer-name pic x(3).
01 print-switch pic 9 value 0.
   88 ready-to-print value 1.
procedure division.
if cust-id in input-record > 0

   move cust-id       in input-record to cust-id       in output-record
   move customer-name in input-record to customer-name in output-record
   set  ready-to-print                to true

end-if
if ready-to-print
   display cust-id in output-record " " customer-name in output-record
end-if
goback.
